#![no_std]
#![no_main]

mod keypad;

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
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
use heapless::String;
use mipidsi::{
    interface::SpiInterface,
    models::ST7789,
    options::{ColorInversion, Orientation, Rotation},
    Builder,
};
use u8g2_fonts::{
    fonts,
    types::{FontColor, HorizontalAlignment, VerticalPosition},
    Content, FontRenderer,
};

use sporo_core::{
    bip39::{self, Mnemonic, WORD_COUNT_TOTAL},
    bip39_wordlist::LetterSet,
    word_entry::{WordEntry, ALPHABET, ALPHABET_TEXT, KEY_DELETE, MAX_WORD_LEN, WORD_COUNT},
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

const LOGO_TEXT: &str = "SPORO";
const INSTRUCTION_TEXT: &str = "press any key";

/// The keypad legend along the bottom of the word screen, and what replaces it
/// once every word is in.
const HINT_TEXT: &str = "4/6 pick  5 add  * del  # next";
const DONE_TEXT: &str = "phrase complete  * to edit";

/// Shown when `#` is pressed on a spelling several words share. Since a spelling
/// that matches nothing cannot be typed, that is the only way to get here, so
/// the instruction is to carry on rather than to correct anything.
const REJECT_TEXT: &str = "several words start so";

const BACKGROUND_COLOR: Rgb565 = Rgb565::BLACK;
const TEXT_COLOR: Rgb565 = Rgb565::WHITE;
const ACCENT_COLOR: Rgb565 = Rgb565::CYAN;
const WARNING_COLOR: Rgb565 = Rgb565::CSS_ORANGE;

/// Letters that would spell something no word starts with. Dim rather than
/// hidden: the alphabet stays a fixed strip, so the letters that are available
/// do not shuffle sideways as the word grows.
const DIM_COLOR: Rgb565 = Rgb565::new(8, 16, 8);

/// Margin kept clear on each side when sizing text to the screen.
const HORIZONTAL_MARGIN: u32 = 8;

/// Gap between the top edge and the progress line on the word screen.
const HEADER_MARGIN: i32 = 4;

/// Distance from the bottom edge up to the middle of the alphabet strip, which
/// leaves the space below it for the hint line.
const ALPHABET_FROM_BOTTOM: i32 = 40;

/// The bar marking the selected letter, measured down from the middle of the
/// strip. Colour alone is too easy to lose at this size.
const CURSOR_BAR_OFFSET: i32 = 8;
const CURSOR_BAR_HEIGHT: u32 = 2;

/// Shape of the finished-phrase grid. Twelve words in six rows of two is the
/// only arrangement that fits 240x135 at a size still worth reading: three
/// columns leaves too little width for an 8-letter word beside its number.
const WORDLIST_ROWS: usize = 6;
const WORDLIST_COLUMNS: usize = WORD_COUNT_TOTAL / WORDLIST_ROWS;

/// Width reserved for the number, wide enough for two digits right-aligned, plus
/// the gap to the word that starts after it.
const WORDLIST_NUMBER_WIDTH: i32 = 13;
const WORDLIST_NUMBER_GAP: i32 = 4;

/// Gap above the first row, and the strip left free at the bottom for the hint.
const WORDLIST_TOP: i32 = 5;
const WORDLIST_HINT_SPACE: i32 = 16;

// Three columns would silently overlap rather than fail to build.
const _: () = assert!(WORDLIST_ROWS * WORDLIST_COLUMNS == WORD_COUNT_TOTAL);

/// Candidate faces for the brand mark, largest first; [`best_fit_font`] picks
/// the biggest that fits. LogiSoSo is a wide geometric sans — deliberately
/// contrasting with the monospace used for body text, the same split the C++
/// screen makes between its `brandFonts` and `monospaceFonts` lists.
static LOGO_FONTS: [FontRenderer; 4] = [
    FontRenderer::new::<fonts::u8g2_font_logisoso42_tr>(),
    FontRenderer::new::<fonts::u8g2_font_logisoso32_tr>(),
    FontRenderer::new::<fonts::u8g2_font_logisoso24_tr>(),
    FontRenderer::new::<fonts::u8g2_font_logisoso16_tr>(),
];

/// Courier, standing in for the Courier Prime Code the C++ UI uses for body
/// text. The last two are narrower fixed faces rather than Courier: a line as
/// long as the alphabet strip (26 cells) or the keypad legend does not fit on
/// 240 pixels in any size of Courier, so without them `best_fit_font` runs out
/// of candidates and overflows the screen.
static BODY_FONTS: [FontRenderer; 5] = [
    FontRenderer::new::<fonts::u8g2_font_courR14_tr>(),
    FontRenderer::new::<fonts::u8g2_font_courR12_tr>(),
    FontRenderer::new::<fonts::u8g2_font_courR10_tr>(),
    FontRenderer::new::<fonts::u8g2_font_7x13_tr>(),
    FontRenderer::new::<fonts::u8g2_font_courR08_tr>(),
];

/// The progress line and the last accepted word are both short and fixed in
/// place, so they get one small face rather than a best-fit list.
static HEADER_FONT: FontRenderer = FontRenderer::new::<fonts::u8g2_font_courR10_tr>();

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

/// The idle screen: brand mark just above centre, instruction pinned to the
/// bottom edge. Modelled on `showHomeScreen` in `../src/screen/tft.cpp`.
fn show_home_screen<D>(display: &mut D)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    display.clear(BACKGROUND_COLOR).expect("clear failed");

    let bounds = display.bounding_box();
    let center = bounds.center();
    let bottom = bounds.size.height as i32;
    let usable_width = bounds.size.width.saturating_sub(HORIZONTAL_MARGIN * 2);

    // Nudged up slightly so the block of text reads as centred once the
    // instruction line at the bottom is taken into account.
    best_fit_font(&LOGO_FONTS, LOGO_TEXT, usable_width)
        .render_aligned(
            LOGO_TEXT,
            Point::new(center.x, center.y - 6),
            VerticalPosition::Center,
            HorizontalAlignment::Center,
            FontColor::Transparent(TEXT_COLOR),
            display,
        )
        .expect("logo render failed");

    best_fit_font(&BODY_FONTS, INSTRUCTION_TEXT, usable_width)
        .render_aligned(
            INSTRUCTION_TEXT,
            Point::new(center.x, bottom - 6),
            VerticalPosition::Bottom,
            HorizontalAlignment::Center,
            FontColor::Transparent(ACCENT_COLOR),
            display,
        )
        .expect("instruction render failed");
}

/// Progress along the top, the word being spelled out across the middle, and 
/// the alphabet the cursor walks below it. Standing in for the recovery-phrase
/// screen the real firmware has.
fn show_word_screen<D>(display: &mut D, entry: &WordEntry)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    display.clear(BACKGROUND_COLOR).expect("clear failed");

    let bounds = display.bounding_box();
    let center = bounds.center();
    let right = bounds.size.width as i32 - HORIZONTAL_MARGIN as i32;
    let bottom = bounds.size.height as i32;
    let usable_width = bounds.size.width.saturating_sub(HORIZONTAL_MARGIN * 2);

    HEADER_FONT
        .render_aligned(
            format_args!("{}/{}", entry.word_number(), WORD_COUNT),
            Point::new(HORIZONTAL_MARGIN as i32, HEADER_MARGIN),
            VerticalPosition::Top,
            HorizontalAlignment::Left,
            FontColor::Transparent(TEXT_COLOR),
            display,
        )
        .expect("progress render failed");

    // The word just accepted, kept in the corner as a check that it went in as
    // intended — `*` reopens it if it didn't. Accepting the eleventh moves
    // straight to the wordlist screen, so this one never renders a complete
    // phrase.
    if let Some(previous) = entry.accepted().last() {
        HEADER_FONT
            .render_aligned(
                previous.as_str(),
                Point::new(right, HEADER_MARGIN),
                VerticalPosition::Top,
                HorizontalAlignment::Right,
                FontColor::Transparent(TEXT_COLOR),
                display,
            )
            .expect("previous word render failed");
    }

    // A refused word is coloured rather than moved or cleared: it is still the
    // word being spelled, and the next keypress carries on from it.
    let word_color = if entry.rejected() {
        WARNING_COLOR
    } else {
        TEXT_COLOR
    };

    draw_word(
        display,
        entry.current(),
        entry.selected(),
        word_color,
        Point::new(center.x, center.y - 12),
        usable_width,
    );
    draw_alphabet(
        display,
        entry.cursor(),
        entry.reachable(),
        center.x,
        bottom - ALPHABET_FROM_BOTTOM,
        usable_width,
    );

    let (hint, hint_color) = if entry.rejected() {
        (REJECT_TEXT, WARNING_COLOR)
    } else {
        (HINT_TEXT, ACCENT_COLOR)
    };

    best_fit_font(&BODY_FONTS, hint, usable_width)
        .render_aligned(
            hint,
            Point::new(center.x, bottom - 6),
            VerticalPosition::Bottom,
            HorizontalAlignment::Center,
            FontColor::Transparent(hint_color),
            display,
        )
        .expect("hint render failed");
}

/// Draws the fresh entropy the final word carries and completes the phrase.
///
/// Called once, on the transition into [`Screen::Wordlist`] — drawing on every
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

/// The finished phrase, numbered, in two columns read top to bottom. The derived
/// word is accented: it is the one the user did not type and cannot check
/// against their own notes.
fn show_wordlist_screen<D>(display: &mut D, mnemonic: &Mnemonic)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    display.clear(BACKGROUND_COLOR).expect("clear failed");

    let bounds = display.bounding_box();
    let center = bounds.center();
    let bottom = bounds.size.height as i32;
    let usable_width = bounds.size.width.saturating_sub(HORIZONTAL_MARGIN * 2);

    let column_width = bounds.size.width as i32 / WORDLIST_COLUMNS as i32;
    let row_height = (bottom - WORDLIST_TOP - WORDLIST_HINT_SPACE) / WORDLIST_ROWS as i32;

    for (position, word) in mnemonic.iter().enumerate() {
        // Column-major: 1-6 down the left, 7-12 down the right.
        let column = position / WORDLIST_ROWS;
        let row = position % WORDLIST_ROWS;

        let left = column as i32 * column_width + HORIZONTAL_MARGIN as i32;
        let y = WORDLIST_TOP + row as i32 * row_height;

        // The number is right-aligned and the word left-aligned from a fixed
        // offset, so the words line up in a column whether the number is one
        // digit or two.
        HEADER_FONT
            .render_aligned(
                format_args!("{}", position + 1),
                Point::new(left + WORDLIST_NUMBER_WIDTH, y),
                VerticalPosition::Top,
                HorizontalAlignment::Right,
                FontColor::Transparent(ACCENT_COLOR),
                display,
            )
            .expect("word number render failed");

        let color = if position == WORD_COUNT_TOTAL - 1 {
            ACCENT_COLOR
        } else {
            TEXT_COLOR
        };

        HEADER_FONT
            .render_aligned(
                *word,
                Point::new(left + WORDLIST_NUMBER_WIDTH + WORDLIST_NUMBER_GAP, y),
                VerticalPosition::Top,
                HorizontalAlignment::Left,
                FontColor::Transparent(color),
                display,
            )
            .expect("word render failed");
    }

    best_fit_font(&BODY_FONTS, DONE_TEXT, usable_width)
        .render_aligned(
            DONE_TEXT,
            Point::new(center.x, bottom - 4),
            VerticalPosition::Bottom,
            HorizontalAlignment::Center,
            FontColor::Transparent(ACCENT_COLOR),
            display,
        )
        .expect("hint render failed");
}

/// Word and preview are sized and centred as one, so the word drifts left as it
/// grows instead of the preview appearing to shove it sideways.
fn draw_word<D>(
    display: &mut D,
    word: &str,
    preview: Option<char>,
    color: Rgb565,
    center: Point,
    usable_width: u32,
) where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    let mut whole: String<{ MAX_WORD_LEN + 1 }> = String::new();
    let _ = whole.push_str(word);
    if let Some(letter) = preview {
        let _ = whole.push(letter);
    }

    let font = best_fit_font(&LOGO_FONTS, whole.as_str(), usable_width);
    let width = font
        .get_rendered_dimensions(whole.as_str(), Point::zero(), VerticalPosition::Center)
        .expect("word measure failed")
        .advance
        .x;

    let mut pen = Point::new(center.x - width / 2, center.y);

    pen.x += font
        .render(
            word,
            pen,
            VerticalPosition::Center,
            FontColor::Transparent(color),
            display,
        )
        .expect("word render failed")
        .advance
        .x;

    if let Some(letter) = preview {
        font.render(
            letter,
            pen,
            VerticalPosition::Center,
            FontColor::Transparent(ACCENT_COLOR),
            display,
        )
        .expect("preview render failed");
    }
}

/// Draws the whole alphabet on one line with the cursor's letter picked out, so
/// the letters either side of it — the ones `4` and `6` reach next — stay
/// visible.
///
/// Letters outside `reachable` are drawn dim: they spell nothing in the
/// wordlist, and `4`/`6` skip straight past them. Keeping them in place rather
/// than dropping them means the strip is the same 26 cells on every screen, so a
/// letter is always where it was last time.
///
/// Rendered a glyph at a time rather than as a string because only that way is
/// there a position to hang the marker under.
fn draw_alphabet<D>(
    display: &mut D,
    cursor: Option<usize>,
    reachable: LetterSet,
    center_x: i32,
    center_y: i32,
    usable_width: u32,
) where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    let font = best_fit_font(&BODY_FONTS, ALPHABET_TEXT, usable_width);
    let width = font
        .get_rendered_dimensions(ALPHABET_TEXT, Point::zero(), VerticalPosition::Center)
        .expect("alphabet measure failed")
        .advance
        .x;

    let mut pen = Point::new(center_x - width / 2, center_y);

    for (index, letter) in ALPHABET.iter().enumerate() {
        let selected = cursor == Some(index);
        let color = match (selected, reachable.contains(index)) {
            (true, _) => ACCENT_COLOR,
            (false, true) => TEXT_COLOR,
            (false, false) => DIM_COLOR,
        };

        let advance = font
            .render(
                *letter as char,
                pen,
                VerticalPosition::Center,
                FontColor::Transparent(color),
                display,
            )
            .expect("alphabet render failed")
            .advance
            .x;

        if selected {
            // The face is monospaced, so the advance is the letter's cell and
            // the bar lines up under it exactly.
            Rectangle::new(
                Point::new(pen.x, center_y + CURSOR_BAR_OFFSET),
                Size::new(advance.max(0) as u32, CURSOR_BAR_HEIGHT),
            )
            .into_styled(PrimitiveStyle::with_fill(ACCENT_COLOR))
            .draw(display)
            .expect("cursor bar render failed");
        }

        pen.x += advance;
    }
}

/// Picks the largest font whose rendering of `text` fits within `max_width`,
/// falling back to the smallest if none do. `fonts` must be ordered largest
/// first. Mirrors `getBestFitFont` in `../src/screen/tft.cpp`.
///
/// Generic over `Content` so a single `char` can be measured without first
/// having to put it in a string.
///
/// Fits against the advance rather than the bounding box: the box covers the
/// inked pixels only, so a run measured that way loses the side bearing of
/// every glyph and a long one — the alphabet strip, say — reports as fitting
/// while rendering off both edges. Advance is also what TFT_eSPI's `textWidth`
/// returns, so this stays equivalent to the C++.
fn best_fit_font<C>(fonts: &[FontRenderer], text: C, max_width: u32) -> &FontRenderer
where
    C: Content + Copy,
{
    fonts
        .iter()
        .find(|font| {
            font.get_rendered_dimensions(text, Point::zero(), VerticalPosition::Center)
                // A missing glyph or an oversized run both mean "not this font".
                .ok()
                .is_some_and(|dimensions| dimensions.advance.x <= max_width as i32)
        })
        .unwrap_or_else(|| fonts.last().expect("font list is never empty"))
}
