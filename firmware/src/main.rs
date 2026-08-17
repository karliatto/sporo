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
    rng::{Trng, TrngSource},
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
    word_entry::{WordEntry, KEY_DELETE},
};
use sporo_ui::{show_home_screen, show_word_screen, show_wordlist_screen, BACKGROUND_COLOR};

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

/// Which screen the keypad is currently talking to.
enum Screen {
    Home,
    Words,
    /// The finished phrase.
    Wordlist,
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

    // The ESP32's RNG only returns true random numbers while a physical noise
    // source is feeding it — the RF subsystem, or the SAR ADC as used here.
    // Without one it degrades to a PRNG, silently, which is the wrong way for a
    // seed generator to fail. Holding `TrngSource` for the rest of `main` keeps
    // the entropy source alive; dropping it would take the guarantee with it.
    let _trng_source = TrngSource::new(peripherals.RNG, peripherals.ADC1);
    let trng = Trng::try_new().expect("TrngSource is alive for the rest of main");

    show_home_screen(&mut display);

    let mut entry = WordEntry::new();
    let mut screen = Screen::Home;
    let mut button_was_down = false;

    // Drawn once per phrase rather than per completion. Going back with `*` to
    // check a word and accepting it again would otherwise draw fresh bits and
    // change the final word — after the user had written it down. The board
    // button starts a new phrase, and takes this with it.
    let mut extra_entropy: Option<u8> = None;

    loop {
        // Redrawing is a full-screen blit, so it only ever happens on an event
        // that actually changed something, never per iteration.
        if let Some(key) = keypad.poll() {
            println!("key pressed: {key}");

            match screen {
                // Any key leaves the home screen, and only leaves it: swallowing
                // the press keeps it from also nudging the cursor off A.
                Screen::Home => {
                    screen = Screen::Words;
                    show_word_screen(&mut display, &entry);
                }
                Screen::Words => {
                    if entry.handle_key(key) {
                        // The last word completes the phrase, so derive the
                        // twelfth here — once, on the transition — and move on.
                        if entry.is_complete() {
                            let extra = *extra_entropy.get_or_insert_with(|| {
                                let mut byte = [0u8; 1];
                                trng.read(&mut byte);
                                byte[0]
                            });

                            let mnemonic = derive_mnemonic(&entry, extra);
                            show_wordlist_screen(&mut display, &mnemonic);
                            screen = Screen::Wordlist;
                        } else {
                            show_word_screen(&mut display, &entry);
                        }
                    }
                }
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

        // `*` is a backspace on the word screen, so starting over is the one
        // thing the keypad can't do; that's what the board button is for.
        // Trigger on the falling edge so holding it down doesn't repeat.
        let button_is_down = button.is_low();
        if button_is_down && !button_was_down {
            println!("button pressed: back to home");
            entry = WordEntry::new();
            extra_entropy = None;
            screen = Screen::Home;
            show_home_screen(&mut display);
        }
        button_was_down = button_is_down;

        delay.delay_millis(2);
    }
}

/// Draws the fresh entropy the final word carries and completes the phrase.
///
/// Called once, on the transition into [`Screen::Wordlist`] — deriving on every
/// redraw would change the last word each time the screen refreshed.
fn derive_mnemonic(entry: &WordEntry, extra_entropy: u8) -> Mnemonic {
    let mnemonic = bip39::complete(entry.accepted(), extra_entropy)
        .expect("every accepted word was resolved against the wordlist");

    // Debug aid for bring-up. A real wallet must never put a seed phrase on a
    // wire: anything with a serial cable attached can read this.
    println!("accepted: {:?}", entry.accepted());
    println!("mnemonic: {:?}", mnemonic);

    mnemonic
}
