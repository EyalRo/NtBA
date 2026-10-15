//! The game's fixed palette, authored the way a 1990 VGA game would: a hardware palette of at
//! most 256 entries (we use under 100), 6 bits per channel, and *ramps* of 4 tones (dark, shadow,
//! base, light) per material so shapes can be shaded by hand-picking from the ramp.

/// Hardware limit we impose: MCGA/VGA mode 13h (320x200, 256 colours).
pub const MAX_COLOURS: usize = 256;

/// Dark, shadow, base, light.
pub type Ramp = [u8; 4];

pub const BLACK: u8 = 0;
pub const WHITE: u8 = 1;

// Four-tone ramps, contiguous from index 2 so `dark_of` can find a ramp's darkest tone.
pub const SKIN_LIGHT: Ramp = [2, 3, 4, 5];
pub const SKIN_MID: Ramp = [6, 7, 8, 9];
pub const SKIN_DARK: Ramp = [10, 11, 12, 13];
pub const GOLD: Ramp = [14, 15, 16, 17];
pub const PURPLE: Ramp = [18, 19, 20, 21];
pub const CLOTH: Ramp = [22, 23, 24, 25]; // white fabric, shoes, socks
pub const STEEL: Ramp = [26, 27, 28, 29];
pub const HAIR_BLACK: Ramp = [30, 31, 32, 33];
pub const HAIR_BROWN: Ramp = [34, 35, 36, 37];
pub const HAIR_BLOND: Ramp = [38, 39, 40, 41];
pub const BALL: Ramp = [42, 43, 44, 45];
pub const RED: Ramp = [46, 47, 48, 49];
pub const GREEN: Ramp = [50, 51, 52, 53];
pub const BLUE: Ramp = [54, 55, 56, 57];
pub const PINK: Ramp = [58, 59, 60, 61];
pub const YELLOW: Ramp = [62, 63, 64, 65];
pub const NAVY: Ramp = [66, 67, 68, 69];
pub const APRON: Ramp = [70, 71, 72, 73];
pub const SUIT: Ramp = [74, 75, 76, 77];
const RAMPS_END: u8 = 78;

/// Hardwood, dark to light.
pub const WOOD: [u8; 6] = [78, 79, 80, 81, 82, 83];
/// Painted lane/centre circle, dark to light.
pub const PAINT: [u8; 3] = [84, 85, 86];
pub const LED: u8 = 87;
pub const LED_DIM: u8 = 88;

pub const SKINS: [Ramp; 3] = [SKIN_LIGHT, SKIN_MID, SKIN_DARK];
pub const HAIRS: [Ramp; 3] = [HAIR_BLACK, HAIR_BROWN, HAIR_BLOND];

/// 8-bit RGB as authored; [`vga`] quantises to the 6-bit DAC a VGA card had.
pub const PALETTE: [[u8; 3]; 89] = [
    [0, 0, 0],
    [252, 252, 252],
    // skin light
    [146, 92, 64],
    [196, 136, 100],
    [236, 182, 140],
    [252, 214, 176],
    // skin mid
    [88, 52, 32],
    [134, 80, 48],
    [180, 116, 70],
    [214, 150, 100],
    // skin dark
    [36, 22, 16],
    [66, 38, 24],
    [100, 60, 38],
    [136, 88, 58],
    // gold
    [128, 88, 8],
    [196, 144, 16],
    [248, 200, 40],
    [255, 236, 128],
    // purple
    [40, 12, 80],
    [76, 30, 132],
    [112, 48, 168],
    [156, 100, 212],
    // cloth
    [104, 104, 124],
    [168, 168, 188],
    [222, 222, 234],
    [252, 252, 252],
    // steel
    [36, 36, 52],
    [84, 84, 104],
    [132, 132, 148],
    [198, 202, 216],
    // hair black
    [8, 8, 12],
    [24, 20, 28],
    [40, 36, 44],
    [72, 64, 76],
    // hair brown
    [32, 16, 8],
    [56, 32, 16],
    [88, 52, 24],
    [120, 76, 40],
    // hair blond
    [136, 100, 32],
    [184, 144, 56],
    [224, 188, 88],
    [248, 224, 136],
    // ball
    [88, 32, 4],
    [176, 72, 12],
    [228, 104, 24],
    [252, 156, 72],
    // red
    [96, 16, 28],
    [156, 32, 44],
    [208, 56, 60],
    [240, 104, 96],
    // green
    [20, 72, 40],
    [40, 120, 64],
    [64, 164, 92],
    [120, 208, 140],
    // blue
    [24, 44, 108],
    [48, 84, 176],
    [84, 128, 224],
    [148, 180, 248],
    // pink
    [104, 32, 88],
    [164, 60, 132],
    [212, 100, 172],
    [240, 152, 208],
    // yellow
    [144, 116, 24],
    [200, 168, 40],
    [240, 208, 72],
    [255, 240, 148],
    // navy
    [8, 8, 28],
    [16, 16, 44],
    [28, 28, 68],
    [44, 44, 96],
    // apron (courtside floor)
    [40, 24, 24],
    [64, 40, 36],
    [88, 56, 48],
    [116, 76, 64],
    // suit
    [16, 16, 24],
    [32, 32, 44],
    [52, 52, 68],
    [84, 84, 104],
    // wood
    [150, 98, 44],
    [172, 116, 56],
    [190, 134, 66],
    [206, 152, 80],
    [222, 170, 98],
    [238, 192, 124],
    // paint
    [24, 40, 112],
    [40, 72, 184],
    [72, 112, 224],
    // LED
    [252, 176, 32],
    [72, 44, 8],
];

/// Quantise an 8-bit channel to the 6 bits a VGA DAC stores, expanded back to 8 bits.
pub const fn vga(channel: u8) -> u8 {
    ((channel >> 2) as u32 * 255 / 63) as u8
}

/// The darkest tone of the ramp `c` belongs to; used for selective sprite outlines.
pub fn dark_of(c: u8) -> u8 {
    if (2..RAMPS_END).contains(&c) {
        2 + (c - 2) / 4 * 4
    } else {
        BLACK
    }
}

/// Shift a ramp tone toward darker (`steps` > 0) or lighter (`steps` < 0), staying in the ramp.
pub fn shift(ramp: &Ramp, tone: usize, steps: i32) -> u8 {
    ramp[(tone as i32 - steps).clamp(0, 3) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_respects_the_vga_limit() {
        assert!(PALETTE.len() <= MAX_COLOURS);
        assert_eq!(PALETTE.len(), LED_DIM as usize + 1);
    }

    #[test]
    fn ramps_are_contiguous_and_darken_toward_index_zero() {
        for ramp in [SKIN_LIGHT, GOLD, STEEL, NAVY, SUIT, BALL] {
            for pair in ramp.windows(2) {
                assert_eq!(pair[1], pair[0] + 1);
            }
            let lum = |c: u8| PALETTE[c as usize].iter().map(|&v| v as u32).sum::<u32>();
            assert!(lum(ramp[0]) < lum(ramp[3]));
            assert_eq!(dark_of(ramp[3]), ramp[0]);
        }
    }

    #[test]
    fn vga_quantisation_hits_the_ends() {
        assert_eq!(vga(0), 0);
        assert_eq!(vga(255), 255);
    }
}
