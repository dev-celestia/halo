//! ANSI / truecolor resolution for the terminal surface.
//!
//! The palette is 40 hardcoded RGB triples plus the xterm 256-colour ramp, and
//! none of it has anything to do with walking the grid.

use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use gpui::{Hsla, hsla};

pub(super) fn resolve_cell_colors(
    fg: Color,
    bg: Color,
    default_fg: Hsla,
    default_bg: Hsla,
    dark: bool,
    bold: bool,
) -> (Hsla, Option<Hsla>) {
    let fg_h = color_to_hsla(fg, default_fg, default_bg, dark, bold);
    let bg_h = match bg {
        Color::Named(NamedColor::Background) => None,
        other => Some(color_to_hsla(other, default_fg, default_bg, dark, false)),
    };
    (fg_h, bg_h)
}

fn color_to_hsla(color: Color, default_fg: Hsla, default_bg: Hsla, dark: bool, bold: bool) -> Hsla {
    match color {
        Color::Named(named) => named_to_hsla(named, default_fg, default_bg, dark, bold),
        Color::Spec(Rgb { r, g, b }) => rgb_to_hsla(r, g, b),
        Color::Indexed(idx) => indexed_to_hsla(idx, default_fg, default_bg, dark, bold),
    }
}

/// 16 ANSI colors — Pro/One Dark on dark surfaces, ink-friendly on light.
fn ansi16(dark: bool) -> [[u8; 3]; 16] {
    if dark {
        [
            [40, 42, 46],    // black
            [224, 108, 117], // red
            [152, 195, 121], // green
            [229, 192, 123], // yellow
            [97, 175, 239],  // blue
            [198, 120, 221], // magenta
            [86, 182, 194],  // cyan
            [171, 178, 191], // white
            [92, 99, 112],   // bright black
            [241, 143, 149], // bright red
            [180, 216, 148], // bright green
            [237, 212, 150], // bright yellow
            [132, 193, 244], // bright blue
            [212, 152, 230], // bright magenta
            [120, 204, 214], // bright cyan
            [255, 255, 255], // bright white
        ]
    } else {
        [
            [36, 38, 41],    // black (ink)
            [180, 48, 48],   // red
            [26, 127, 84],   // green
            [158, 112, 16],  // yellow (readable on paper)
            [36, 99, 180],   // blue
            [148, 48, 148],  // magenta
            [16, 128, 148],  // cyan
            [90, 94, 100],   // white → mid ink (not near-paper)
            [120, 124, 130], // bright black
            [200, 64, 58],   // bright red
            [32, 148, 96],   // bright green
            [180, 132, 24],  // bright yellow
            [48, 120, 200],  // bright blue
            [168, 64, 168],  // bright magenta
            [24, 148, 168],  // bright cyan
            [36, 38, 41],    // bright white → primary ink
        ]
    }
}

fn named_to_hsla(
    named: NamedColor,
    default_fg: Hsla,
    default_bg: Hsla,
    dark: bool,
    bold: bool,
) -> Hsla {
    match named {
        NamedColor::Foreground => default_fg,
        NamedColor::Background => default_bg,
        NamedColor::Cursor => default_fg,
        other => {
            let mut idx = match other {
                NamedColor::Black => 0,
                NamedColor::Red => 1,
                NamedColor::Green => 2,
                NamedColor::Yellow => 3,
                NamedColor::Blue => 4,
                NamedColor::Magenta => 5,
                NamedColor::Cyan => 6,
                NamedColor::White => 7,
                NamedColor::BrightBlack => 8,
                NamedColor::BrightRed => 9,
                NamedColor::BrightGreen => 10,
                NamedColor::BrightYellow => 11,
                NamedColor::BrightBlue => 12,
                NamedColor::BrightMagenta => 13,
                NamedColor::BrightCyan => 14,
                NamedColor::BrightWhite => 15,
                _ => return default_fg,
            };
            // Classic xterm: bold + colors 0–7 → bright 8–15.
            if bold && idx < 8 {
                idx += 8;
            }
            let [r, g, b] = ansi16(dark)[idx];
            rgb_to_hsla(r, g, b)
        }
    }
}

fn indexed_to_hsla(idx: u8, default_fg: Hsla, default_bg: Hsla, dark: bool, bold: bool) -> Hsla {
    match idx {
        0..=15 => {
            let named = match idx {
                0 => NamedColor::Black,
                1 => NamedColor::Red,
                2 => NamedColor::Green,
                3 => NamedColor::Yellow,
                4 => NamedColor::Blue,
                5 => NamedColor::Magenta,
                6 => NamedColor::Cyan,
                7 => NamedColor::White,
                8 => NamedColor::BrightBlack,
                9 => NamedColor::BrightRed,
                10 => NamedColor::BrightGreen,
                11 => NamedColor::BrightYellow,
                12 => NamedColor::BrightBlue,
                13 => NamedColor::BrightMagenta,
                14 => NamedColor::BrightCyan,
                _ => NamedColor::BrightWhite,
            };
            named_to_hsla(named, default_fg, default_bg, dark, bold)
        }
        16..=231 => {
            let n = idx - 16;
            let r = n / 36;
            let g = (n % 36) / 6;
            let b = n % 6;
            let ramp = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            rgb_to_hsla(ramp(r), ramp(g), ramp(b))
        }
        232..=255 => {
            let gray = 8 + (idx - 232) * 10;
            rgb_to_hsla(gray, gray, gray)
        }
    }
}

pub(super) fn rgb_to_hsla(r: u8, g: u8, b: u8) -> Hsla {
    let rf = r as f32 / 255.;
    let gf = g as f32 / 255.;
    let bf = b as f32 / 255.;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.;
    if (max - min).abs() < f32::EPSILON {
        return hsla(0., 0., l, 1.);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2. - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - rf).abs() < f32::EPSILON {
        let v = (gf - bf) / d + if gf < bf { 6. } else { 0. };
        v / 6.
    } else if (max - gf).abs() < f32::EPSILON {
        ((bf - rf) / d + 2.) / 6.
    } else {
        ((rf - gf) / d + 4.) / 6.
    };
    hsla(h, s, l, 1.)
}
