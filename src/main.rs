#![no_std]
#![no_main]

mod keypad;

use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
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

const LOGO_TEXT: &str = "SPORE";
const INSTRUCTION_TEXT: &str = "press any key";

const BACKGROUND_COLOR: Rgb565 = Rgb565::BLACK;
const TEXT_COLOR: Rgb565 = Rgb565::WHITE;
const ACCENT_COLOR: Rgb565 = Rgb565::CYAN;

/// Margin kept clear on each side when sizing text to the screen.
const HORIZONTAL_MARGIN: u32 = 8;

/// How many recent keys the strip along the bottom of the key screen holds
/// before starting over.
const KEY_BUFFER_CAPACITY: usize = 16;

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

/// Courier, standing in for the Courier Prime Code the C++ UI uses for body text.
static BODY_FONTS: [FontRenderer; 3] = [
    FontRenderer::new::<fonts::u8g2_font_courR14_tr>(),
    FontRenderer::new::<fonts::u8g2_font_courR12_tr>(),
    FontRenderer::new::<fonts::u8g2_font_courR10_tr>(),
];

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let mut delay = Delay::new();

    println!("Spore starting");

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

    let mut recent: String<KEY_BUFFER_CAPACITY> = String::new();
    let mut button_was_down = false;

    loop {
        // Any key leaves the home screen; from then on each press just replaces
        // what's shown. Redrawing is a full-screen blit, so it only ever happens
        // on an actual event, never per iteration.
        if let Some(key) = keypad.poll() {
            println!("key pressed: {key}");

            // Rather than scroll, start over once the strip is full — this is a
            // readout for checking the wiring, not the firmware's amount buffer.
            if recent.push(key).is_err() {
                recent.clear();
                let _ = recent.push(key);
            }

            show_key_screen(&mut display, key, &recent);
        }

        // The keypad has no "back" key here — `*` is the reset key in the C++
        // firmware, but this screen has to be able to show it like any other.
        // Trigger on the falling edge so holding the button doesn't repeat.
        let button_is_down = button.is_low();
        if button_is_down && !button_was_down {
            println!("button pressed: back to home");
            recent.clear();
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

/// The screen reached from home by pressing a key: the key just pressed, large
/// and centred, over a strip of the keys pressed before it. Standing in for the
/// amount-entry screen the real firmware moves on to.
fn show_key_screen<D>(display: &mut D, key: char, recent: &str)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    display.clear(BACKGROUND_COLOR).expect("clear failed");

    let bounds = display.bounding_box();
    let center = bounds.center();
    let bottom = bounds.size.height as i32;
    let usable_width = bounds.size.width.saturating_sub(HORIZONTAL_MARGIN * 2);

    best_fit_font(&LOGO_FONTS, key, usable_width)
        .render_aligned(
            key,
            Point::new(center.x, center.y - 6),
            VerticalPosition::Center,
            HorizontalAlignment::Center,
            FontColor::Transparent(ACCENT_COLOR),
            display,
        )
        .expect("key render failed");

    // `best_fit_font` shrinks the face as the strip fills, so this stays within
    // the margins without any wrapping logic.
    best_fit_font(&BODY_FONTS, recent, usable_width)
        .render_aligned(
            recent,
            Point::new(center.x, bottom - 6),
            VerticalPosition::Bottom,
            HorizontalAlignment::Center,
            FontColor::Transparent(TEXT_COLOR),
            display,
        )
        .expect("recent keys render failed");
}

/// Picks the largest font whose rendering of `text` fits within `max_width`,
/// falling back to the smallest if none do. `fonts` must be ordered largest
/// first. Mirrors `getBestFitFont` in `../src/screen/tft.cpp`.
///
/// Generic over `Content` so a single `char` can be measured without first
/// having to put it in a string.
fn best_fit_font<C>(fonts: &[FontRenderer], text: C, max_width: u32) -> &FontRenderer
where
    C: Content + Copy,
{
    fonts
        .iter()
        .find(|font| {
            font.get_rendered_dimensions_aligned(
                text,
                Point::zero(),
                VerticalPosition::Center,
                HorizontalAlignment::Center,
            )
            // A missing glyph or an oversized run both mean "not this font".
            .ok()
            .flatten()
            .is_some_and(|bbox| bbox.size.width <= max_width)
        })
        .unwrap_or_else(|| fonts.last().expect("font list is never empty"))
}
