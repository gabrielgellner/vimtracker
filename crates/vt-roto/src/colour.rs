//! The Roto's colour schemes, Appendix A of both APIs: each a background in RGB, with black or
//! white text on it. A track's, a control's and a switch's light are each an index into this
//! table, so a colour of the program's own goes through [`nearest`]. The first 70 are Ableton
//! Live's clip and track colours, which its script sends as they are; the rest are plain ones.
//! Adapted from Melbourne Instruments' API documents (see `NOTICE`), with its one misprint, `BO`
//! for `B0` in scheme 47, put right.

use crate::bytes::Rgb;

/// One colour scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scheme {
    pub background: Rgb,
    /// Whether its text is white rather than black.
    pub white_text: bool,
}

impl Scheme {
    const fn new(red: u8, green: u8, blue: u8, white_text: bool) -> Self {
        Self {
            background: Rgb::new(red, green, blue),
            white_text,
        }
    }
}

/// Every scheme, by its index, `00` to `52`.
pub const SCHEMES: [Scheme; 83] = [
    Scheme::new(0xFF, 0x94, 0xA0, false),
    Scheme::new(0xFF, 0xA5, 0x27, false),
    Scheme::new(0xD3, 0x99, 0x25, false),
    Scheme::new(0xFF, 0xF4, 0x77, false),
    Scheme::new(0xC6, 0xFB, 0x00, false),
    Scheme::new(0x1B, 0xFF, 0x2D, false),
    Scheme::new(0x26, 0xFF, 0xA2, false),
    Scheme::new(0x5F, 0xFF, 0xDF, false),
    Scheme::new(0x90, 0xC5, 0xF6, false),
    Scheme::new(0x57, 0x80, 0xDB, false),
    Scheme::new(0x97, 0xA7, 0xF6, false),
    Scheme::new(0xE0, 0x6C, 0xDB, false),
    Scheme::new(0xED, 0x53, 0x9A, false),
    Scheme::new(0xFF, 0xFF, 0xF6, false),
    Scheme::new(0xFF, 0x36, 0x34, false),
    Scheme::new(0xFF, 0x6C, 0x02, false),
    Scheme::new(0x9E, 0x62, 0x48, false),
    Scheme::new(0xFF, 0xF0, 0x32, false),
    Scheme::new(0x8C, 0xFF, 0x63, false),
    Scheme::new(0x3F, 0xC3, 0x00, false),
    Scheme::new(0x00, 0xBF, 0xA8, false),
    Scheme::new(0x19, 0xE9, 0xF6, false),
    Scheme::new(0x10, 0xA4, 0xE5, false),
    Scheme::new(0x00, 0x7D, 0xB9, false),
    Scheme::new(0x8D, 0x6C, 0xDB, false),
    Scheme::new(0xBD, 0x77, 0xBF, false),
    Scheme::new(0xFF, 0x39, 0xCC, false),
    Scheme::new(0xD8, 0xD0, 0xC8, false),
    Scheme::new(0xEC, 0x68, 0x56, false),
    Scheme::new(0xFF, 0xA3, 0x6F, false),
    Scheme::new(0xDB, 0xAD, 0x6D, false),
    Scheme::new(0xF6, 0xFF, 0xA7, false),
    Scheme::new(0xDA, 0xE4, 0x92, false),
    Scheme::new(0xC1, 0xD0, 0x6F, false),
    Scheme::new(0xA1, 0xC4, 0x88, false),
    Scheme::new(0xDC, 0xFD, 0xD9, false),
    Scheme::new(0xD4, 0xF1, 0xEF, false),
    Scheme::new(0xC0, 0xC1, 0xDA, false),
    Scheme::new(0xD4, 0xBB, 0xDB, false),
    Scheme::new(0xB4, 0x98, 0xDC, false),
    Scheme::new(0xED, 0xDC, 0xD9, false),
    Scheme::new(0xAF, 0xA9, 0xA3, false),
    Scheme::new(0xEE, 0x92, 0x86, false),
    Scheme::new(0xBE, 0x82, 0x52, false),
    Scheme::new(0x9E, 0x83, 0x66, false),
    Scheme::new(0xC6, 0xBA, 0x65, false),
    Scheme::new(0xAC, 0xBE, 0x00, false),
    Scheme::new(0x81, 0xB0, 0x4A, false),
    Scheme::new(0x8D, 0xC2, 0xB3, false),
    Scheme::new(0xA1, 0xB3, 0xBD, false),
    Scheme::new(0x8A, 0xA5, 0xBF, false),
    Scheme::new(0x88, 0x93, 0xC4, false),
    Scheme::new(0xAB, 0x95, 0xAE, false),
    Scheme::new(0xC6, 0x9F, 0xB7, false),
    Scheme::new(0xC3, 0x71, 0x90, false),
    Scheme::new(0x7F, 0x7B, 0x76, false),
    Scheme::new(0xB5, 0x33, 0x31, true),
    Scheme::new(0xAF, 0x51, 0x2F, true),
    Scheme::new(0x76, 0x4F, 0x3E, true),
    Scheme::new(0xE3, 0xC3, 0x00, true),
    Scheme::new(0x8A, 0x96, 0x1D, true),
    Scheme::new(0x56, 0x9F, 0x2F, true),
    Scheme::new(0x0A, 0x9C, 0x88, true),
    Scheme::new(0x24, 0x63, 0x7F, true),
    Scheme::new(0x1B, 0x2F, 0x90, true),
    Scheme::new(0x30, 0x52, 0x9C, true),
    Scheme::new(0x65, 0x4B, 0xA6, true),
    Scheme::new(0xA9, 0x4B, 0xA6, true),
    Scheme::new(0xD3, 0x2E, 0x6A, true),
    Scheme::new(0x3E, 0x3C, 0x39, true),
    Scheme::new(0x10, 0x10, 0x10, true),
    Scheme::new(0xFF, 0x00, 0x00, true),
    Scheme::new(0x00, 0xFF, 0x00, false),
    Scheme::new(0xFF, 0xFF, 0x00, false),
    Scheme::new(0x00, 0x00, 0xF6, true),
    Scheme::new(0xFF, 0x00, 0xF6, true),
    Scheme::new(0x00, 0xFF, 0xF6, false),
    Scheme::new(0x84, 0x00, 0x00, true),
    Scheme::new(0x84, 0x80, 0x00, false),
    Scheme::new(0x00, 0x80, 0x00, true),
    Scheme::new(0x00, 0x80, 0x7B, false),
    Scheme::new(0x00, 0x00, 0x7B, true),
    Scheme::new(0x84, 0x00, 0x7B, true),
];

/// The scheme closest to a colour, by distance in RGB weighted for how the eye sees each part.
pub fn nearest(colour: Rgb) -> u8 {
    let distance = |scheme: &Scheme| {
        let part = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
        let background = scheme.background;
        2 * part(colour.red, background.red)
            + 4 * part(colour.green, background.green)
            + 3 * part(colour.blue, background.blue)
    };
    SCHEMES
        .iter()
        .enumerate()
        .min_by_key(|(_, scheme)| distance(scheme))
        .map_or(0, |(index, _)| index as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_colour_finds_its_scheme() {
        assert_eq!(nearest(Rgb::new(0xFF, 0x00, 0x00)), 71);
        assert_eq!(nearest(Rgb::new(0x81, 0xB0, 0x4A)), 47);
    }

    #[test]
    fn the_last_scheme_is_0x52() {
        assert_eq!(SCHEMES.len() - 1, 0x52);
    }
}
