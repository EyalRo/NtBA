//! Input is a bitmask so it is trivial to ship over a wire (future P2P) and to replay.

pub const UP: u8 = 1 << 0;
pub const DOWN: u8 = 1 << 1;
pub const LEFT: u8 = 1 << 2;
pub const RIGHT: u8 = 1 << 3;
pub const PASS: u8 = 1 << 4;
pub const SHOOT: u8 = 1 << 5;
pub const SWITCH: u8 = 1 << 6;
pub const START: u8 = 1 << 7;

/// Movement direction encoded in `mask` as a (not normalised) vector.
pub fn dir(mask: u8) -> (f32, f32) {
    let mut x = 0.0;
    let mut y = 0.0;
    if mask & LEFT != 0 {
        x -= 1.0;
    }
    if mask & RIGHT != 0 {
        x += 1.0;
    }
    if mask & UP != 0 {
        y -= 1.0;
    }
    if mask & DOWN != 0 {
        y += 1.0;
    }
    (x, y)
}
