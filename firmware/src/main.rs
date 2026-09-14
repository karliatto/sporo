#![no_std]
#![no_main]

mod keypad;

use embedded_graphics::prelude::*;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pin as _, Pull},
    main,
    spi::{
        master::{Config as SpiConfig, Spi},
        Mode,
    },
    time::Rate,
};
use esp_println::println;
use mipidsi::{
    interface::SpiInterface,
    models::ST7789,
    options::{ColorInversion, Orientation, Rotation},
    Builder,
};

use sporo_core::{
    bip39::{self, Mnemonic},
    coin_entry::{CoinEntry, CoinEvent},
    word_entry::{WordEntry, KEY_DELETE},
};
use sporo_ui::{
    show_about_screen, show_coin_screen, show_home_screen, show_menu_screen, show_word_screen,
    show_wordlist_screen, Menu, MenuEvent, MenuItem, BACKGROUND_COLOR,
};

use crate::keypad::Keypad;

// Required by the ESP-IDF second-stage bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

/// The T-Display exposes a 135x240 window into the ST7789's 240x320 framebuffer,
/// so every write has to be shifted by this offset. Given `Rotation::Deg90`
/// below, the drawable area ends up 240 wide by 135 tall.
const DISPLAY_WIDTH: u16 = 135;
const DISPLAY_HEIGHT: u16 = 240;
const DISPLAY_OFFSET_X: u16 = 52;
const DISPLAY_OFFSET_Y: u16 = 40;

/// Bytes of pixel data batched per SPI transfer. Bigger is faster, up to a point.
const SPI_BUFFER_SIZE: usize = 512;

const FIRMWARE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Which screen the keypad is currently talking to.
enum Screen {
    Home,
    /// Picking what the device should do.
    Menu,
    Words,
    /// The coin flips that finish the phrase.
    Coin,
    /// The finished phrase.
    Wordlist,
    /// Firmware version and the shape of the phrase it builds.
    About,
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let mut delay = Delay::new();

    println!("Sporo starting");

    // Keep the backlight off until the panel is initialised, so the user doesn't
    // see the ST7789's power-on garbage. Active high on this board.
    let mut backlight = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());

    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(40))
            .with_mode(Mode::_0),
    )
    .expect("invalid SPI configuration")
    .with_sck(peripherals.GPIO18)
    .with_mosi(peripherals.GPIO19);

    let cs = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    let rst = Output::new(peripherals.GPIO23, Level::High, OutputConfig::default());

    // mipidsi wants an `SpiDevice` (a bus plus chip-select); esp-hal gives us a
    // bare `SpiBus`. This is the standard adapter for a bus with one device.
    let spi_device = ExclusiveDevice::new(spi, cs, delay).expect("failed to drive CS high");

    let mut spi_buffer = [0u8; SPI_BUFFER_SIZE];
    let interface = SpiInterface::new(spi_device, dc, &mut spi_buffer);

    let mut display = Builder::new(ST7789, interface)
        .reset_pin(rst)
        .display_size(DISPLAY_WIDTH, DISPLAY_HEIGHT)
        .display_offset(DISPLAY_OFFSET_X, DISPLAY_OFFSET_Y)
        // This panel is wired such that colours come out inverted otherwise.
        .invert_colors(ColorInversion::Inverted)
        .orientation(Orientation::new().rotate(Rotation::Deg90))
        .init(&mut delay)
        .expect("display init failed");

    display.clear(BACKGROUND_COLOR).expect("clear failed");
    backlight.set_high();

    let size = display.bounding_box().size;
    println!("display up: {}x{}", size.width, size.height);

    // Upper button on the T-Display, active low. Note GPIO0 is also a strapping
    // pin: holding it down during reset puts the chip into download mode.
    let button = Input::new(
        peripherals.GPIO0,
        InputConfig::default().with_pull(Pull::Up),
    );

    let mut keypad = Keypad::new(
        [
            peripherals.GPIO21.degrade(),
            peripherals.GPIO27.degrade(),
            peripherals.GPIO26.degrade(),
            peripherals.GPIO22.degrade(),
        ],
        [
            peripherals.GPIO33.degrade(),
            peripherals.GPIO32.degrade(),
            peripherals.GPIO25.degrade(),
        ],
    );

    show_home_screen(&mut display);

    let mut entry = WordEntry::new();
    let mut menu = Menu::new();
    let mut screen = Screen::Home;
    let mut button_was_down = false;

    // Kept for the life of the phrase rather than per completion. Going back
    // with `*` to check a word and accepting it again lands on the coin screen
    // with the same seven flips still on it, so the final word does not change
    // under a user who has already written it down — and, unlike the byte the
    // chip's RNG used to supply, they can see for themselves that it did not.
    // The board button starts a new phrase, and takes these with it.
    let mut coins = CoinEntry::new();

    loop {
        // Redrawing is a full-screen blit, so it only ever happens on an event
        // that actually changed something, never per iteration.
        if let Some(key) = keypad.poll() {
            println!("key pressed: {key}");

            match screen {
                // Any key leaves the home screen, and only leaves it: swallowing
                // the press keeps it from also nudging the menu cursor off the
                // first entry.
                Screen::Home => {
                    screen = Screen::Menu;
                    show_menu_screen(&mut display, &menu);
                }
                Screen::Menu => match menu.handle_key(key) {
                    MenuEvent::Moved => show_menu_screen(&mut display, &menu),
                    MenuEvent::Chose(MenuItem::GenerateMnemonic) => {
                        screen = Screen::Words;
                        show_word_screen(&mut display, &entry);
                    }
                    MenuEvent::Chose(MenuItem::About) => {
                        screen = Screen::About;
                        show_about_screen(&mut display, FIRMWARE_VERSION);
                    }
                    MenuEvent::Dismissed => {
                        screen = Screen::Home;
                        show_home_screen(&mut display);
                    }
                    MenuEvent::Ignored => {}
                },
                // Nothing to pick; `*` is the way back, as it is everywhere else.
                Screen::About => {
                    if key == KEY_DELETE {
                        screen = Screen::Menu;
                        show_menu_screen(&mut display, &menu);
                    }
                }
                Screen::Words => {
                    // Once, into a binding: the arm below branches on this and
                    // asking twice would delete two letters for one press.
                    let changed = entry.handle_key(key);

                    // `*` that changed nothing is the entry state's only
                    // unambiguous "backed out of the first word", and it is
                    // otherwise inert. Route it to the menu, so leaving does not
                    // need the board button, which wipes the phrase.
                    if key == KEY_DELETE && !changed {
                        screen = Screen::Menu;
                        show_menu_screen(&mut display, &menu);
                    } else if changed {
                        // The eleventh word is the last the keypad can spell.
                        // The seven bits the twelfth carries come off the coin
                        // screen, which opens here.
                        if entry.is_complete() {
                            screen = Screen::Coin;
                            show_coin_screen(&mut display, &coins);
                        } else {
                            show_word_screen(&mut display, &entry);
                        }
                    }
                }
                Screen::Coin => match coins.handle_key(key) {
                    CoinEvent::Changed => show_coin_screen(&mut display, &coins),
                    CoinEvent::Confirmed => {
                        let mnemonic = derive_mnemonic(&entry, &coins);
                        show_wordlist_screen(&mut display, &mnemonic);
                        screen = Screen::Wordlist;
                    }
                    // `*` past the last flip is the way back, and reopens the
                    // last word — the same press, and the same effect, as `*`
                    // on the wordlist screen. Landing on a word screen reading
                    // 11/11 with no letter selectable would be a dead end.
                    CoinEvent::Dismissed => {
                        entry.handle_key(KEY_DELETE);
                        screen = Screen::Words;
                        show_word_screen(&mut display, &entry);
                    }
                    CoinEvent::Ignored => {}
                },
                // Nothing to pick here; `*` is the way back, reopening the last
                // word for editing exactly as it does on the entry screen.
                Screen::Wordlist => {
                    if key == KEY_DELETE && entry.handle_key(KEY_DELETE) {
                        screen = Screen::Words;
                        show_word_screen(&mut display, &entry);
                    }
                }
            }
        }

        // `*` on the word screen deletes one letter at a time, so the keypad can
        // only abandon a phrase by backing out of every word in it. The board
        // button drops the lot in one press, from wherever the user is.
        // Trigger on the falling edge so holding it down doesn't repeat.
        let button_is_down = button.is_low();
        if button_is_down && !button_was_down {
            println!("button pressed: back to home");
            entry = WordEntry::new();
            // The cursor goes back to the first entry too: this is the "start
            // over" button, and resuming on whatever was last picked is not that.
            menu = Menu::new();
            coins = CoinEntry::new();
            screen = Screen::Home;
            show_home_screen(&mut display);
        }
        button_was_down = button_is_down;

        delay.delay_millis(2);
    }
}

/// Completes the phrase from the entered words and the user's coin flips.
///
/// Called on the transition into [`Screen::Wordlist`]. The flips are kept for
/// the life of the phrase, so coming back here after checking a word derives the
/// same twelfth word from the same seven bits.
fn derive_mnemonic(entry: &WordEntry, coins: &CoinEntry) -> Mnemonic {
    let extra_entropy = coins
        .entropy()
        .expect("the coin screen only confirms once every flip is in");

    let mnemonic = bip39::complete(entry.accepted(), extra_entropy)
        .expect("every accepted word was resolved against the wordlist");

    // Debug aid for bring-up. A real wallet must never put a seed phrase on a
    // wire: anything with a serial cable attached can read this.
    println!("accepted: {:?}", entry.accepted());
    println!("mnemonic: {:?}", mnemonic);

    mnemonic
}
