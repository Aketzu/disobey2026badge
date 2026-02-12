//! Name tag example — displays a name scaled to fill the screen.
//!
//! Set the name at compile time via the `NAME` environment variable.
//! Optionally set `BG` and `FG` as 6-char hex RGB colors (or `BG="rainbow"`
//! for an animated hue-cycling background), and `LEDS=heartbeat` or
//! `LEDS=rainbow` for LED effects.
//!
//! ```sh
//! NAME="User" BG="rainbow" FG="E0E0E0" LEDS="heartbeat" cargo run --release --example nametag
//! NAME="Admin" BG="rainbow" FG="E00000" LEDS="rainbow" cargo run --release --example nametag
//! NAME="Speaker" cargo run --release --example nametag
//! ```

#![no_std]
#![no_main]

use defmt::info;
#[allow(clippy::wildcard_imports)]
use disobey2026badge::*;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::Rectangle,
};
use esp_backtrace as _;
use esp_hal::timer::timg::TimerGroup;
use esp_println as _;
use palette::Srgb;

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

const NAME: Option<&str> = option_env!("NAME");
const DEFAULT_NAME: Option<&str> = Some("Anonymous Alpaca");
const LEDS: Option<&str> = option_env!("LEDS");
const BG_STR: Option<&str> = option_env!("BG");
const FG_STR: Option<&str> = option_env!("FG");
const W: u32 = 320;
const H: u32 = 170;

/// Parse a hex color string like "FF8800" into Rgb565 at const time.
/// Returns None if the string is not exactly 6 hex chars.
const fn parse_hex_rgb565(s: &str) -> Option<Rgb565> {
    let b = s.as_bytes();
    if b.len() != 6 {
        return None;
    }
    let Some(r) = hex_byte(b[0], b[1]) else { return None };
    let Some(g) = hex_byte(b[2], b[3]) else { return None };
    let Some(b) = hex_byte(b[4], b[5]) else { return None };
    // Rgb565: 5 bits red, 6 bits green, 5 bits blue
    Some(Rgb565::new(r >> 3, g >> 2, b >> 3))
}

const fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

const fn hex_byte(hi: u8, lo: u8) -> Option<u8> {
    let Some(h) = hex_digit(hi) else { return None };
    let Some(l) = hex_digit(lo) else { return None };
    Some(h << 4 | l)
}

/// Const-compatible string equality check.
const fn str_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const BG_RAINBOW: bool = match BG_STR {
    Some(s) => str_eq(s, "rainbow"),
    None => false,
};

const BG_COLOR: Rgb565 = match BG_STR {
    Some(s) if BG_RAINBOW => Rgb565::BLACK,
    Some(s) => match parse_hex_rgb565(s) {
        Some(c) => c,
        None => panic!("BG must be a 6-char hex RGB string or \"rainbow\""),
    },
    None => Rgb565::new(2, 8, 20),
};

const FG_COLOR: Rgb565 = match FG_STR {
    Some(s) => match parse_hex_rgb565(s) {
        Some(c) => c,
        None => panic!("FG must be a 6-char hex RGB string, e.g. \"FFFFFF\""),
    },
    None => Rgb565::WHITE,
};

// 5×7 bitmap font — each character is 5 columns, 7 rows, stored as 7 bytes
// where bits 4..0 represent the columns (bit 4 = leftmost).
const GLYPH_W: u32 = 5;
const GLYPH_H: u32 = 7;
const GLYPH_GAP: u32 = 1; // 1-column gap between characters

/// Returns the 5×7 glyph data for a character, or None if unsupported.
fn glyph(ch: char) -> Option<[u8; 7]> {
    match ch {
        'A' | 'a' => Some([0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
        'B' | 'b' => Some([0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110]),
        'C' | 'c' => Some([0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110]),
        'D' | 'd' => Some([0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110]),
        'E' | 'e' => Some([0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111]),
        'F' | 'f' => Some([0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000]),
        'G' | 'g' => Some([0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110]),
        'H' | 'h' => Some([0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
        'I' | 'i' => Some([0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
        'J' | 'j' => Some([0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100]),
        'K' | 'k' => Some([0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001]),
        'L' | 'l' => Some([0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111]),
        'M' | 'm' => Some([0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001]),
        'N' | 'n' => Some([0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001]),
        'O' | 'o' => Some([0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
        'P' | 'p' => Some([0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000]),
        'Q' | 'q' => Some([0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101]),
        'R' | 'r' => Some([0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001]),
        'S' | 's' => Some([0b01110, 0b10001, 0b10000, 0b01110, 0b00001, 0b10001, 0b01110]),
        'T' | 't' => Some([0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
        'U' | 'u' => Some([0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
        'V' | 'v' => Some([0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100]),
        'W' | 'w' => Some([0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001]),
        'X' | 'x' => Some([0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001]),
        'Y' | 'y' => Some([0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100]),
        'Z' | 'z' => Some([0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111]),
        '0' => Some([0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110]),
        '1' => Some([0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
        '2' => Some([0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111]),
        '3' => Some([0b01110, 0b10001, 0b00001, 0b00110, 0b00001, 0b10001, 0b01110]),
        '4' => Some([0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010]),
        '5' => Some([0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110]),
        '6' => Some([0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110]),
        '7' => Some([0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000]),
        '8' => Some([0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]),
        '9' => Some([0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110]),
        ' ' => Some([0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000]),
        '-' => Some([0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000]),
        '.' => Some([0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100]),
        '_' => Some([0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111]),
        _ => None,
    }
}

/// Convert a hue (0..360) to an Rgb565 color at full saturation and given value.
fn hue_to_rgb565(hue: f32, value: f32) -> Rgb565 {
    let c = value;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (hue as u16) / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Rgb565::new((r * 31.0) as u8, (g * 63.0) as u8, (b * 31.0) as u8)
}

/// Precomputed name layout for fast per-pixel rendering.
struct NameLayout {
    scale: u32,
    offset_x: i32,
    offset_y: i32,
    char_count: u32,
}

impl NameLayout {
    fn compute() -> Self {
        let char_count = NAME.or(DEFAULT_NAME).unwrap().chars().count() as u32;
        if char_count == 0 {
            return Self { scale: 1, offset_x: 0, offset_y: 0, char_count: 0 };
        }

        let margin = 10u32;
        let available_w = W - margin * 2;
        let available_h = H - margin * 2 - 30;

        let text_w = char_count * GLYPH_W + (char_count - 1) * GLYPH_GAP;
        let text_h = GLYPH_H;

        let scale_x = available_w / text_w;
        let scale_y = available_h / text_h;
        let scale = if scale_x < scale_y { scale_x } else { scale_y };
        let scale = if scale < 1 { 1 } else { scale };

        let scaled_w = text_w * scale;
        let scaled_h = text_h * scale;
        let offset_x = ((W - scaled_w) / 2) as i32;
        let offset_y = ((H - 30 - scaled_h) / 2) as i32;

        Self { scale, offset_x, offset_y, char_count }
    }

    /// Check if pixel (px, py) is a foreground (glyph) pixel.
    fn is_fg(&self, px: i32, py: i32) -> bool {
        if self.char_count == 0 {
            return false;
        }
        // Check if pixel is within the scaled text bounding box
        let rx = px - self.offset_x;
        let ry = py - self.offset_y;
        if rx < 0 || ry < 0 {
            return false;
        }
        let rx = rx as u32;
        let ry = ry as u32;
        let total_h = GLYPH_H * self.scale;
        if ry >= total_h {
            return false;
        }

        // Which glyph row (0..GLYPH_H) and which character?
        let glyph_row = ry / self.scale;
        let char_stride = (GLYPH_W + GLYPH_GAP) * self.scale;

        let char_idx = rx / char_stride;
        let within_char = rx % char_stride;

        if char_idx >= self.char_count {
            return false;
        }
        // Within the gap between characters?
        if within_char >= GLYPH_W * self.scale {
            return false;
        }
        let glyph_col = within_char / self.scale;

        // Look up the character
        
        if let Some(ch) = NAME.or(DEFAULT_NAME).unwrap().chars().nth(char_idx as usize) {
            if let Some(rows) = glyph(ch) {
                return (rows[glyph_row as usize] >> (GLYPH_W - 1 - glyph_col)) & 1 == 1;
            }
        }
        false
    }
}

const LABEL: &str = "DISOBEY 2026";
const LABEL_SCALE: u32 = 2;
const LABEL_COLOR: Rgb565 = Rgb565::new(16, 32, 16);

/// Check if pixel (px, py) is part of the bottom label, returning its color.
fn is_label_pixel(px: i32, py: i32) -> bool {
    let label_len = LABEL.len() as u32;
    let label_w = label_len * GLYPH_W * LABEL_SCALE + (label_len - 1) * GLYPH_GAP * LABEL_SCALE;
    let label_h = GLYPH_H * LABEL_SCALE;
    let label_x = ((W - label_w) / 2) as i32;
    // baseline at H - 10 means top of glyphs is at H - 10 - label_h (approx)
    let label_y = H as i32 - 10 - label_h as i32;

    let rx = px - label_x;
    let ry = py - label_y;
    if rx < 0 || ry < 0 {
        return false;
    }
    let rx = rx as u32;
    let ry = ry as u32;
    if ry >= label_h {
        return false;
    }

    let glyph_row = ry / LABEL_SCALE;
    let char_stride = (GLYPH_W + GLYPH_GAP) * LABEL_SCALE;
    let char_idx = rx / char_stride;
    let within_char = rx % char_stride;

    if char_idx >= label_len {
        return false;
    }
    if within_char >= GLYPH_W * LABEL_SCALE {
        return false;
    }
    let glyph_col = within_char / LABEL_SCALE;

    if let Some(rows) = glyph(LABEL.as_bytes()[char_idx as usize] as char) {
        return (rows[glyph_row as usize] >> (GLYPH_W - 1 - glyph_col)) & 1 == 1;
    }
    false
}

/// Draw the full frame in a single `fill_contiguous` call — no flicker.
fn draw_frame(display: &mut Display, bg: Rgb565, layout: &NameLayout) {
    let area = Rectangle::new(Point::zero(), Size::new(W, H));
    let pixels = (0u32..(W * H)).map(|i| {
        let px = (i % W) as i32;
        let py = (i / W) as i32;
        if layout.is_fg(px, py) {
            FG_COLOR
        } else if is_label_pixel(px, py) {
            LABEL_COLOR
        } else {
            bg
        }
    });
    display.fill_contiguous(&area, pixels).unwrap();
}

#[embassy_executor::task]
async fn display_task(
    display: &'static mut disobey2026badge::Display<'static>,
    backlight: &'static mut Backlight,
) {
    info!("Name tag: {}", NAME);
    backlight.on();

    let layout = NameLayout::compute();

    if BG_RAINBOW {
        let mut hue = 0u16;
        loop {
            let bg = hue_to_rgb565(hue as f32, 0.4);
            draw_frame(display, bg, &layout);
            hue = (hue + 2) % 360;
            Timer::after(Duration::from_millis(50)).await;
        }
    } else {
        draw_frame(display, BG_COLOR, &layout);
        loop {
            Timer::after(Duration::from_secs(600)).await;
        }
    }
}


#[embassy_executor::task]
async fn heartbeat_task(leds: &'static mut Leds<'static>) {
    info!("Heartbeat LED task started");
    let off = Srgb::new(0u8, 0, 0);

    loop {
        // Double-beat pattern like a real heartbeat: lub-dub ... pause
        for &(brightness, ms) in &[
            // First beat (lub)
            (30u8, 80u64),
            (10, 100),
            // Second beat (dub)
            (30, 80),
            (5, 120),
            // Pause
            (0, 600),
        ] {
            let color = Srgb::new(brightness, 0, 0);
            leds.fill(color);
            leds.update().await;
            Timer::after(Duration::from_millis(ms)).await;
        }
        leds.fill(off);
        leds.update().await;
    }
}

#[embassy_executor::task]
async fn rainbow_task(leds: &'static mut Leds<'static>) {
    info!("Rainbow LED task started");

    let mut offset = 0u16;
    loop {
        for i in 0..leds.len() {
            let hue = ((offset + i as u16 * 25) % 360) as f32;
            // Simple HSV→RGB with S=1, V=0.08 (dim)
            let c = 0.08_f32;
            let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
            let (r, g, b) = match (hue as u16) / 60 {
                0 => (c, x, 0.0),
                1 => (x, c, 0.0),
                2 => (0.0, c, x),
                3 => (0.0, x, c),
                4 => (x, 0.0, c),
                _ => (c, 0.0, x),
            };
            leds.set(i, Srgb::new((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8));
        }
        leds.update().await;
        offset = (offset + 3) % 360;
        Timer::after(Duration::from_millis(50)).await;
    }
}



#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = disobey2026badge::init();
    let resources = split_resources!(peripherals);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    let display = mk_static!(disobey2026badge::Display<'static>, resources.display.into());
    let backlight = mk_static!(Backlight, resources.backlight.into());
    spawner.must_spawn(display_task(display, backlight));

    match LEDS {
        Some("heartbeat") => {
            let leds = mk_static!(Leds<'static>, resources.leds.into());
            spawner.must_spawn(heartbeat_task(leds));
        }
        Some("rainbow") => {
            let leds = mk_static!(Leds<'static>, resources.leds.into());
            spawner.must_spawn(rainbow_task(leds));
        }
        _ => {}
    }

    loop {
        Timer::after(Duration::from_secs(600)).await;
    }
}
