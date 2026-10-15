//! Tiny shaded-sprite toolkit. Shapes are rasterised into a small paletted canvas with a light
//! from the top-left, picking tones from a 4-step ramp, then given a selective 1-pixel outline
//! (each edge takes the darkest tone of the colour it borders), like hand-drawn sprites of the era.

use crate::font::{glyph, text_width, GLYPH_W};
use crate::palette::{dark_of, Ramp};

pub const CLEAR: u8 = 255;
/// Direction light comes *from* (screen space, y down): top-left.
const LIGHT: (f32, f32) = (-0.55, -0.65);

pub struct Sprite {
    w: i32,
    h: i32,
    ox: i32,
    oy: i32,
    pix: Vec<u8>,
}

impl Sprite {
    /// A `w`x`h` canvas whose drawing origin (feet centre, usually) is at (`ox`, `oy`).
    pub fn new(w: i32, h: i32, ox: i32, oy: i32) -> Self {
        Sprite {
            w,
            h,
            ox,
            oy,
            pix: vec![CLEAR; (w * h) as usize],
        }
    }

    fn set(&mut self, x: i32, y: i32, c: u8) {
        let (x, y) = (x + self.ox, y + self.oy);
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.pix[(y * self.w + x) as usize] = c;
        }
    }

    fn get(&self, x: i32, y: i32) -> u8 {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.pix[(y * self.w + x) as usize]
        } else {
            CLEAR
        }
    }

    pub fn put(&mut self, x: i32, y: i32, c: u8) {
        self.set(x, y, c);
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u8) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }

    /// Pick a tone from `ramp` for a surface whose normal is `(nx, ny)`; `bias` < 0 darkens
    /// (used for limbs on the far side of the body).
    fn tone(nx: f32, ny: f32, bias: f32, ramp: &Ramp) -> u8 {
        let l = nx * LIGHT.0 + ny * LIGHT.1 + bias;
        if l > 0.38 {
            ramp[3]
        } else if l > -0.2 {
            ramp[2]
        } else {
            ramp[1]
        }
    }

    /// Shaded ellipse centred at (`cx`, `cy`) relative to the origin.
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, bias: f32, ramp: &Ramp) {
        for y in (cy - ry - 1.0).floor() as i32..=(cy + ry + 1.0).ceil() as i32 {
            for x in (cx - rx - 1.0).floor() as i32..=(cx + rx + 1.0).ceil() as i32 {
                let (nx, ny) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
                if nx * nx + ny * ny <= 1.0 {
                    self.set(x, y, Self::tone(nx, ny, bias, ramp));
                }
            }
        }
    }

    /// Shaded capsule (a rounded limb) of radius `r` along the segment (`x0`,`y0`)-(`x1`,`y1`).
    #[allow(clippy::too_many_arguments)] // plain geometry: two endpoints, radius, shading
    pub fn capsule(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, bias: f32, ramp: &Ramp) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = (dx * dx + dy * dy).max(0.0001);
        let (minx, maxx) = (x0.min(x1) - r - 1.0, x0.max(x1) + r + 1.0);
        let (miny, maxy) = (y0.min(y1) - r - 1.0, y0.max(y1) + r + 1.0);
        for y in miny.floor() as i32..=maxy.ceil() as i32 {
            for x in minx.floor() as i32..=maxx.ceil() as i32 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = (((px - x0) * dx + (py - y0) * dy) / len2).clamp(0.0, 1.0);
                let (qx, qy) = (px - (x0 + t * dx), py - (y0 + t * dy));
                if qx * qx + qy * qy <= r * r {
                    self.set(x, y, Self::tone(qx / r, qy / r, bias, ramp));
                }
            }
        }
    }

    /// Replace every pixel of `ramp`'s tones inside the rectangle with `to` (for stripes etc.).
    #[allow(clippy::too_many_arguments)] // rectangle, stride, source ramp, target colour
    pub fn recolour_rows(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        every: i32,
        from: &Ramp,
        to: u8,
    ) {
        for yy in y..y + h {
            if (yy - y) % every != 0 {
                continue;
            }
            for xx in x..x + w {
                if from.contains(&self.get(xx + self.ox, yy + self.oy)) {
                    self.set(xx, yy, to);
                }
            }
        }
    }

    pub fn text(&mut self, x: i32, y: i32, s: &str, c: u8) {
        let mut cx = x;
        for ch in s.chars() {
            for (row, bits) in glyph(ch).iter().enumerate() {
                for col in 0..GLYPH_W {
                    if bits & (1 << (GLYPH_W - 1 - col)) != 0 {
                        self.set(cx + col, y + row as i32, c);
                    }
                }
            }
            cx += GLYPH_W + 1;
        }
    }

    pub fn text_width(s: &str) -> i32 {
        text_width(s, 1)
    }

    /// One-pixel selective outline around everything drawn so far.
    pub fn outline(&mut self) {
        let mut out = self.pix.clone();
        for y in 0..self.h {
            for x in 0..self.w {
                if self.pix[(y * self.w + x) as usize] != CLEAR {
                    continue;
                }
                for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                    let n = self.get(x + dx, y + dy);
                    if n != CLEAR {
                        out[(y * self.w + x) as usize] = dark_of(n);
                        break;
                    }
                }
            }
        }
        self.pix = out;
    }

    /// Plot the sprite with its origin at (`sx`, `sy`).
    pub fn blit(&self, sx: i32, sy: i32, mut plot: impl FnMut(i32, i32, u8)) {
        for y in 0..self.h {
            for x in 0..self.w {
                let c = self.pix[(y * self.w + x) as usize];
                if c != CLEAR {
                    plot(sx + x - self.ox, sy + y - self.oy, c);
                }
            }
        }
    }
}
