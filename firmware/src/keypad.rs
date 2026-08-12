//! Scanner for the 4x3 matrix keypad.
//!
//! Replaces the Arduino `Keypad` library the main firmware uses; there is no
//! `no_std` equivalent, but a matrix scan is short enough to write directly
//! against `esp_hal::gpio`. Pins and layout mirror the defaults in
//! `../src/config.cpp` (`keypadRowPins`, `keypadColPins`, `keypadCharList`).

use esp_hal::{
    delay::Delay,
    gpio::{AnyPin, Flex, Input, InputConfig, OutputConfig, Pull},
    time::{Duration, Instant},
};

pub const ROWS: usize = 4;
pub const COLS: usize = 3;

/// Row-major, left-to-right and top-to-bottom, which is how the Arduino library
/// indexes its keymap and therefore how `keypadCharList` is written.
const KEYMAP: [[char; COLS]; ROWS] = [
    ['1', '2', '3'],
    ['4', '5', '6'],
    ['7', '8', '9'],
    ['*', '0', '#'],
];

/// Matches `setDebounceTime(10)` in the library this replaces: it is both the
/// debounce window and the minimum interval between hardware scans.
const SCAN_INTERVAL: Duration = Duration::from_millis(10);

/// Time given to the row lines to settle after a column starts driving. The
/// rows are held up by weak internal pull-ups, so they fall slowly; reading
/// too early sees the previous column's result.
const SETTLE_MICROS: u32 = 5;

pub struct Keypad<'d> {
    rows: [Input<'d>; ROWS],
    cols: [Flex<'d>; COLS],
    pressed: [[bool; COLS]; ROWS],
    last_scan: Instant,
    delay: Delay,
}

impl<'d> Keypad<'d> {
    /// Rows become pull-up inputs; columns become outputs latched low but left
    /// high-impedance until they're selected.
    ///
    /// Takes [`AnyPin`] rather than concrete pins because every GPIO is its own
    /// type, and these have to live in arrays. Call `.degrade()` on each.
    pub fn new(row_pins: [AnyPin<'d>; ROWS], col_pins: [AnyPin<'d>; COLS]) -> Self {
        let rows = row_pins.map(|pin| Input::new(pin, InputConfig::default().with_pull(Pull::Up)));

        let cols = col_pins.map(|pin| {
            let mut col = Flex::new(pin);
            col.apply_output_config(&OutputConfig::default());
            // Nothing ever reads a column, and leaving the input buffer on
            // while the line floats wastes power.
            col.set_input_enable(false);
            col.set_low();
            col.set_output_enable(false);

            col
        });

        Self {
            rows,
            cols,
            pressed: [[false; COLS]; ROWS],
            last_scan: Instant::now(),
            delay: Delay::new(),
        }
    }

    /// Returns a key only on the transition into pressed, so holding a key down
    /// does not repeat. Cheap to call in a tight loop: it scans at most once per
    /// [`SCAN_INTERVAL`] and does nothing in between.
    pub fn poll(&mut self) -> Option<char> {
        if self.last_scan.elapsed() < SCAN_INTERVAL {
            return None;
        }
        self.last_scan = Instant::now();

        let current = self.scan();
        let mut key = None;

        for row in 0..ROWS {
            for col in 0..COLS {
                if current[row][col] && !self.pressed[row][col] && key.is_none() {
                    key = Some(KEYMAP[row][col]);
                }
            }
        }

        self.pressed = current;

        key
    }

    /// Pulls each column low in turn and reads the rows, which are active low
    /// because a pressed key shorts its row to the driven column.
    fn scan(&mut self) -> [[bool; COLS]; ROWS] {
        let mut pressed = [[false; COLS]; ROWS];

        for (col_index, col) in self.cols.iter_mut().enumerate() {
            // The level is already latched low, so enabling the driver is all
            // it takes to select this column.
            col.set_output_enable(true);
            self.delay.delay_micros(SETTLE_MICROS);

            for (row_index, row) in self.rows.iter().enumerate() {
                pressed[row_index][col_index] = row.is_low();
            }

            // Back to high-impedance. Without this, pressing two keys in the
            // same row would short one column's driver against another's.
            col.set_output_enable(false);
        }

        pressed
    }
}
