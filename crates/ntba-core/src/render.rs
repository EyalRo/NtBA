//! Software renderer: draws one frame of the game into a 320x200 paletted
//! framebuffer, then expands it to RGBA for the host to blit.
//!
//! The look follows the late-80s/early-90s originals: a raised sideline view that scrolls to
//! follow the ball (half a court on screen), strong perspective toward the near sideline, multi-tone
//! hardwood planks, a painted lane, a 3/4-view hoop with a shot clock on the stanchion, benches and
//! a scorer's table behind the far sideline, and an amber dot-matrix scoreboard.

use crate::font::{glyph, text_width, GLYPH_W};
use crate::game::{BallState, Game, Phase, HOOP_Y, RIM_Z};
use crate::{COURT_L, COURT_W, SCREEN_H, SCREEN_W};

// Palette indices. 32 colours; the Genesis could show 64, the DOS version 16.
pub const BLACK: u8 = 0;
pub const WHITE: u8 = 1;
pub const WOOD_LIGHT: u8 = 2;
pub const WOOD_MID: u8 = 3;
pub const WOOD_DARK: u8 = 4;
pub const WOOD_HI: u8 = 5;
pub const PAINT: u8 = 6;
pub const PAINT_DARK: u8 = 7;
pub const SKIN_LIGHT: u8 = 8;
pub const SKIN_MID: u8 = 9;
pub const SKIN_DARK: u8 = 10;
pub const HAIR_BLOND: u8 = 11;
pub const JERSEY: u8 = 12;
pub const TRIM: u8 = 13;
pub const GRAY: u8 = 14;
pub const DARK: u8 = 15;
pub const WALL: u8 = 16;
pub const SHADOW: u8 = 17;
pub const BALL: u8 = 18;
pub const BALL_SEAM: u8 = 19;
pub const LED: u8 = 20;
pub const LED_DIM: u8 = 21;
pub const CROWD_RED: u8 = 22;
pub const CROWD_GREEN: u8 = 23;
pub const CROWD_BLUE: u8 = 24;
pub const CROWD_PINK: u8 = 25;
pub const CROWD_YELLOW: u8 = 26;
pub const SUIT: u8 = 27;
pub const STEEL: u8 = 28;
pub const APRON: u8 = 29;
pub const SEATS: u8 = 30;
pub const JERSEY_HI: u8 = 31;

/// RGB for each palette index.
pub const PALETTE: [[u8; 3]; 32] = [
    [0, 0, 0],
    [252, 252, 252],
    [220, 168, 96],
    [200, 144, 72],
    [172, 116, 56],
    [236, 188, 116],
    [40, 72, 184],
    [24, 40, 112],
    [244, 196, 152],
    [200, 132, 84],
    [120, 72, 40],
    [244, 216, 104],
    [248, 200, 40],
    [112, 48, 168],
    [136, 136, 148],
    [64, 64, 80],
    [24, 24, 56],
    [140, 92, 44],
    [232, 104, 24],
    [120, 48, 8],
    [252, 176, 32],
    [72, 44, 8],
    [196, 52, 52],
    [52, 148, 76],
    [60, 96, 200],
    [200, 80, 160],
    [232, 208, 72],
    [36, 36, 48],
    [200, 204, 216],
    [72, 44, 40],
    [140, 32, 40],
    [255, 236, 140],
];

// Screen layout.
const COURT_TOP: i32 = 84;
const COURT_PX_H: i32 = 100;
const PX_PER_FT_Y: f32 = COURT_PX_H as f32 / COURT_W;
const Z_PX: f32 = 4.3; // screen pixels per foot of height
const CAM_MIN: f32 = 28.0;
const CAM_MAX: f32 = COURT_L - 28.0;
const FAR_SCALE: f32 = 3.6; // pixels per foot along x at the far sideline
const NEAR_SCALE: f32 = 6.4; // ... and at the near sideline

pub struct Framebuffer {
    pub idx: Vec<u8>,
    pub rgba: Vec<u8>,
    cam_x: f32,
    cam_ready: bool,
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Framebuffer {
    pub fn new() -> Self {
        Framebuffer {
            idx: vec![0; SCREEN_W * SCREEN_H],
            rgba: vec![255; SCREEN_W * SCREEN_H * 4],
            cam_x: COURT_L / 2.0,
            cam_ready: false,
        }
    }

    fn px(&mut self, x: i32, y: i32, c: u8) {
        if x >= 0 && y >= 0 && (x as usize) < SCREEN_W && (y as usize) < SCREEN_H {
            self.idx[y as usize * SCREEN_W + x as usize] = c;
        }
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u8) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.px(xx, yy, c);
            }
        }
    }

    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u8) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.px(x, y, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn text(&mut self, x: i32, y: i32, s: &str, c: u8, scale: i32) {
        let mut cx = x;
        for ch in s.chars() {
            let g = glyph(ch);
            for (row, bits) in g.iter().enumerate() {
                for col in 0..GLYPH_W {
                    if bits & (1 << (GLYPH_W - 1 - col)) != 0 {
                        self.rect(cx + col * scale, y + row as i32 * scale, scale, scale, c);
                    }
                }
            }
            cx += (GLYPH_W + 1) * scale;
        }
    }

    fn text_centered(&mut self, cx: i32, y: i32, s: &str, c: u8, scale: i32) {
        self.text(cx - text_width(s, scale) / 2, y, s, c, scale);
    }

    /// Amber dot-matrix text: every font pixel is a 2x2 lamp on a 3-pixel pitch, unlit lamps
    /// stay faintly visible like on a real scoreboard.
    fn dot_text(&mut self, x: i32, y: i32, s: &str) {
        let mut cx = x;
        for ch in s.chars() {
            let g = glyph(ch);
            for (row, bits) in g.iter().enumerate() {
                for col in 0..GLYPH_W {
                    let lit = bits & (1 << (GLYPH_W - 1 - col)) != 0;
                    self.rect(
                        cx + col * 3,
                        y + row as i32 * 3,
                        2,
                        2,
                        if lit { LED } else { LED_DIM },
                    );
                }
            }
            cx += 12;
        }
    }

    fn expand(&mut self) {
        for (i, &c) in self.idx.iter().enumerate() {
            let rgb = PALETTE[c as usize % PALETTE.len()];
            self.rgba[i * 4] = rgb[0];
            self.rgba[i * 4 + 1] = rgb[1];
            self.rgba[i * 4 + 2] = rgb[2];
            self.rgba[i * 4 + 3] = 255;
        }
    }

    /// Ease the scrolling camera toward the action (visual only; never feeds the simulation).
    fn update_camera(&mut self, g: &Game) {
        let (fx, lead) = match g.ball.state {
            BallState::Held(h) => (g.players[h].x, g.players[h].facing as f32 * 6.0),
            _ => (g.ball.x, 0.0),
        };
        let target = (fx + lead).clamp(CAM_MIN, CAM_MAX);
        if self.cam_ready {
            self.cam_x += (target - self.cam_x) * 0.1;
        } else {
            self.cam_x = target;
            self.cam_ready = true;
        }
    }
}

/// Horizontal pixels per foot at court depth `y` (perspective: nearer = bigger).
fn x_scale(y: f32) -> f32 {
    FAR_SCALE + (NEAR_SCALE - FAR_SCALE) * (y / COURT_W)
}

/// Project a court position (feet) and height into screen pixels for a camera at `cam_x`.
pub fn project(cam_x: f32, x: f32, y: f32, z: f32) -> (i32, i32) {
    let sx = 160.0 + (x - cam_x) * x_scale(y);
    let sy = COURT_TOP as f32 + y * PX_PER_FT_Y - z * Z_PX;
    (sx.round() as i32, sy.round() as i32)
}

pub fn draw(g: &Game, fb: &mut Framebuffer) {
    fb.update_camera(g);
    let cam = fb.cam_x;
    fb.rect(0, 0, SCREEN_W as i32, SCREEN_H as i32, APRON);
    draw_crowd(fb, cam, g.ticks);
    draw_bench_area(fb, cam);
    draw_court(fb, cam);
    for mirror in [false, true] {
        draw_hoop_back(fb, cam, mirror, g.shot_clock);
    }
    draw_entities(g, fb, cam);
    for mirror in [false, true] {
        draw_rim_and_net(fb, cam, mirror);
    }
    draw_bottom_strip(fb);
    draw_scoreboard(g, fb);
    draw_overlays(g, fb);
    fb.expand();
}

fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

// ---------------------------------------------------------------- arena

fn draw_crowd(fb: &mut Framebuffer, cam: f32, ticks: u64) {
    fb.rect(0, 0, SCREEN_W as i32, 56, WALL);
    let scroll = (cam * 2.0) as i32; // parallax: the stands move slower than the floor
    let (base, off) = (scroll.div_euclid(6), scroll.rem_euclid(6));
    let bodies = [
        CROWD_RED,
        CROWD_GREEN,
        CROWD_BLUE,
        CROWD_PINK,
        CROWD_YELLOW,
        JERSEY,
        WHITE,
        TRIM,
    ];
    let skins = [SKIN_LIGHT, SKIN_MID, SKIN_DARK, SKIN_LIGHT];
    for row in 0..7 {
        let y0 = 4 + row * 7;
        for col in 0..=(SCREEN_W as i32 / 6 + 1) {
            let wc = base + col;
            let x = col * 6 - off;
            if wc.rem_euclid(13) == 0 {
                // aisle stairs
                fb.rect(x, y0, 6, 7, DARK);
                fb.rect(x, y0 + 3, 6, 1, GRAY);
                continue;
            }
            let h = hash(wc as u32, row as u32);
            let cheer = if (h >> 4) % 6 == 0 {
                ((ticks / 14 + (h as u64 & 7)) & 1) as i32
            } else {
                0
            };
            fb.rect(x + 1, y0 + 3 - cheer, 5, 4, bodies[(h % 8) as usize]);
            let hair = if (h >> 9) % 4 == 0 { HAIR_BLOND } else { BLACK };
            fb.rect(x + 2, y0 - cheer, 3, 3, skins[((h >> 7) % 4) as usize]);
            fb.rect(x + 2, y0 - cheer, 3, 1, hair);
            if cheer == 1 && (h >> 12) & 1 == 0 {
                fb.rect(x + 5, y0 - 3, 1, 3, skins[((h >> 7) % 4) as usize]);
            }
        }
    }
    // Wall band and advertising strip above the benches.
    fb.rect(0, 54, SCREEN_W as i32, 3, DARK);
    fb.rect(0, 57, SCREEN_W as i32, 9, SEATS);
    let ad_off = (cam * 3.0) as i32;
    for k in -1..12 {
        let x = k * 40 - ad_off.rem_euclid(40);
        fb.rect(x, 59, 30, 5, PAINT_DARK);
        fb.rect(x + 2, 60, 26, 3, PAINT);
    }
}

struct Look {
    skin: u8,
    hair: u8,
    top: u8,
    trim: u8,
    shorts: u8,
    sock: u8,
    number: u8,
    stripes: bool,
}

const TEAM_LOOKS: [Look; 5] = [
    Look {
        skin: SKIN_MID,
        hair: BLACK,
        top: JERSEY,
        trim: TRIM,
        shorts: TRIM,
        sock: WHITE,
        number: 4,
        stripes: false,
    },
    Look {
        skin: SKIN_LIGHT,
        hair: HAIR_BLOND,
        top: JERSEY,
        trim: TRIM,
        shorts: TRIM,
        sock: WHITE,
        number: 12,
        stripes: false,
    },
    Look {
        skin: SKIN_DARK,
        hair: BLACK,
        top: JERSEY,
        trim: TRIM,
        shorts: TRIM,
        sock: WHITE,
        number: 23,
        stripes: false,
    },
    Look {
        skin: SKIN_MID,
        hair: BLACK,
        top: JERSEY,
        trim: TRIM,
        shorts: TRIM,
        sock: WHITE,
        number: 32,
        stripes: false,
    },
    Look {
        skin: SKIN_DARK,
        hair: BLACK,
        top: JERSEY,
        trim: TRIM,
        shorts: TRIM,
        sock: WHITE,
        number: 45,
        stripes: false,
    },
];
const REF_LOOK: Look = Look {
    skin: SKIN_MID,
    hair: BLACK,
    top: WHITE,
    trim: BLACK,
    shorts: BLACK,
    sock: BLACK,
    number: 0,
    stripes: true,
};

/// A seated person behind the far sideline (bench or table). Feet row is `sy`.
fn draw_seated(fb: &mut Framebuffer, sx: i32, sy: i32, jacket: u8, skin: u8, hair: u8, suit: bool) {
    fb.rect(sx - 4, sy - 15, 9, 11, GRAY); // folding chair back
    fb.rect(sx - 3, sy - 13, 7, 8, jacket);
    if suit {
        fb.rect(sx - 1, sy - 13, 3, 4, WHITE);
        fb.px(sx, sy - 12, CROWD_RED);
        fb.px(sx, sy - 11, CROWD_RED);
    } else {
        fb.rect(sx - 1, sy - 13, 1, 8, WHITE); // zip stripe
    }
    fb.rect(sx - 2, sy - 18, 5, 5, skin);
    fb.rect(sx - 2, sy - 18, 5, 2, hair);
    fb.rect(sx - 3, sy - 5, 3, 4, if suit { SUIT } else { jacket });
    fb.rect(sx + 1, sy - 5, 3, 4, if suit { SUIT } else { jacket });
    fb.rect(sx - 3, sy - 1, 3, 2, WHITE);
    fb.rect(sx + 1, sy - 1, 3, 2, WHITE);
}

fn draw_bench_area(fb: &mut Framebuffer, cam: f32) {
    fb.rect(0, 66, SCREEN_W as i32, 18, APRON);
    let feet_y = 80;
    // Benches: home on the left in gold warm-ups, visitors on the right in green.
    for (k, x) in [8.0_f32, 11.5, 15.0, 18.5, 22.0].iter().enumerate() {
        let (sx, _) = project(cam, *x, -1.5, 0.0);
        let look = &TEAM_LOOKS[k];
        draw_seated(fb, sx, feet_y, JERSEY, look.skin, look.hair, false);
    }
    for (k, x) in [66.0_f32, 69.5, 73.0, 76.5, 80.0].iter().enumerate() {
        let (sx, _) = project(cam, *x, -1.5, 0.0);
        let skin = [SKIN_LIGHT, SKIN_DARK, SKIN_MID, SKIN_DARK, SKIN_LIGHT][k];
        draw_seated(fb, sx, feet_y, CROWD_GREEN, skin, BLACK, false);
    }
    for x in [25.5_f32, 29.0, 83.5, 87.0] {
        let (sx, _) = project(cam, x, -1.5, 0.0);
        draw_seated(fb, sx, feet_y, SUIT, SKIN_LIGHT, GRAY, true);
    }
    // Scorer's table with officials behind it.
    let (x0, _) = project(cam, 38.0, -1.5, 0.0);
    let (x1, _) = project(cam, 56.0, -1.5, 0.0);
    for (k, x) in [40.0_f32, 44.0, 48.0, 52.0].iter().enumerate() {
        let (sx, _) = project(cam, *x, -1.5, 0.0);
        let (jacket, skin) = if k == 1 {
            (WHITE, SKIN_MID)
        } else {
            (SUIT, [SKIN_LIGHT, SKIN_MID, SKIN_DARK, SKIN_LIGHT][k])
        };
        draw_seated(fb, sx, 76, jacket, skin, BLACK, k != 1);
    }
    fb.rect(x0, 67, x1 - x0, 2, WHITE);
    fb.rect(x0, 69, x1 - x0, 11, PAINT);
    let mut x = x0;
    while x < x1 {
        fb.rect(x, 69, 1, 11, PAINT_DARK);
        x += 5;
    }
    let mut x = x0;
    while x < x1 {
        fb.rect(x, 80, 3, 1, PAINT);
        x += 6;
    }
    for mx in [x0 + 8, x0 + 26, x0 + 46] {
        fb.rect(mx, 62, 7, 5, BLACK);
        fb.rect(mx + 1, 63, 5, 3, GRAY);
    }
    fb.rect(x0 + 14, 66, 6, 1, WHITE);
}

// ---------------------------------------------------------------- court

fn near(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() < tol
}

/// Colour of the hardwood at a court position (feet), markings included. `s` is the local
/// horizontal pixels-per-foot so line widths stay about two pixels.
fn floor_colour(wx: f32, wy: f32, s: f32) -> u8 {
    let mx = wx.min(COURT_L - wx); // everything but the centre line is symmetric
    let ltx = 1.0 / s; // half line width along x (feet) = 1px
    let lty = 1.0 / (2.0 * PX_PER_FT_Y); // half line width along y (feet) = 1px
    let hoop_d = ((mx - 5.25).powi(2) + (wy - HOOP_Y).powi(2)).sqrt();
    let ft_d = ((mx - 19.0).powi(2) + (wy - HOOP_Y).powi(2)).sqrt();
    let centre_d = ((wx - COURT_L / 2.0).powi(2) + (wy - HOOP_Y).powi(2)).sqrt();

    let line = wy < 2.0 * lty
        || wy > COURT_W - 2.0 * lty
        || mx < 2.0 * ltx
        || near(wx, COURT_L / 2.0, ltx)
        || near(centre_d, 6.0, 0.55)
        || (mx < 19.0 && (near(wy, 17.0, lty) || near(wy, 33.0, lty)))
        || (near(mx, 19.0, ltx) && (wy - HOOP_Y).abs() < 8.0)
        || (mx >= 19.0 && near(ft_d, 6.0, 0.55))
        || (near(hoop_d, 23.75, 0.55) && (wy - HOOP_Y).abs() < 22.0 && mx > 5.25)
        || (mx < 14.0 && (near(wy, 3.0, lty) || near(wy, COURT_W - 3.0, lty)));
    if line {
        return BLACK;
    }
    if (mx < 19.0 && (wy - HOOP_Y).abs() < 8.0) || centre_d < 6.0 {
        return PAINT;
    }
    // Hardwood: horizontal planks, each with its own length, offset and tone.
    let row = wy as i32;
    let h = hash(row as u32, 7);
    let len = 6.0 + (h % 6) as f32;
    let off = ((h >> 8) % 97) as f32;
    let t = (wx + off) / len;
    let plank = t.floor() as i32;
    if t - t.floor() < 0.5 / len {
        return WOOD_DARK; // butt joint between boards
    }
    match hash(row as u32, plank as u32 ^ 0xABCD) % 7 {
        0 | 1 | 6 => WOOD_MID,
        2 | 3 => WOOD_LIGHT,
        4 => WOOD_DARK,
        _ => WOOD_HI,
    }
}

fn draw_court(fb: &mut Framebuffer, cam: f32) {
    for sy in COURT_TOP..COURT_TOP + COURT_PX_H {
        let wy = (sy - COURT_TOP) as f32 / PX_PER_FT_Y;
        let s = x_scale(wy);
        for sx in 0..SCREEN_W as i32 {
            let wx = cam + (sx as f32 - 160.0) / s;
            if (0.0..=COURT_L).contains(&wx) {
                fb.px(sx, sy, floor_colour(wx, wy, s));
            }
        }
    }
}

// ---------------------------------------------------------------- hoop

fn hoop_pt(cam: f32, mirror: bool, x: f32, y: f32, z: f32) -> (i32, i32) {
    project(cam, if mirror { COURT_L - x } else { x }, y, z)
}

/// Stanchion, support arm, backboard (seen at an angle) and shot clock.
fn draw_hoop_back(fb: &mut Framebuffer, cam: f32, mirror: bool, shot_clock: f32) {
    let p = |x: f32, y: f32, z: f32| hoop_pt(cam, mirror, x, y, z);
    // Padded base and pole behind the baseline.
    let (bx, by) = p(-3.5, HOOP_Y, 0.0);
    fb.rect(bx - 5, by - 16, 10, 16, PAINT);
    fb.rect(bx - 5, by - 16, 3, 16, PAINT_DARK);
    let (px, py) = p(-3.5, HOOP_Y, 9.5);
    fb.rect(px - 1, py, 3, by - 16 - py, STEEL);
    // Arm to the backboard.
    let (ax, ay) = p(1.5, HOOP_Y, 9.5);
    fb.line(px, py, ax, ay, STEEL);
    fb.line(px, py + 1, ax, ay + 1, GRAY);
    // Backboard: a vertical pane from y=22..28 ft, z=9..13 ft.
    let steps = 18;
    for i in 0..=steps {
        let y = 22.0 + 6.0 * i as f32 / steps as f32;
        let (sx, top) = p(1.5, y, 13.0);
        let (_, bot) = p(1.5, y, 9.0);
        fb.line(sx, top, sx, bot, WHITE);
        if i == 0 || i == steps {
            fb.line(sx, top, sx, bot, STEEL);
        }
    }
    let (l, lt) = p(1.5, 22.0, 13.0);
    let (r, rt) = p(1.5, 28.0, 13.0);
    let (_, lb) = p(1.5, 22.0, 9.0);
    let (_, rb) = p(1.5, 28.0, 9.0);
    fb.line(l, lt, r, rt, STEEL);
    fb.line(l, lb, r, rb, STEEL);
    // Target square.
    let (t0, t0y) = p(1.5, 24.2, 11.6);
    let (t1, t1y) = p(1.5, 25.8, 11.6);
    let (_, b0y) = p(1.5, 24.2, 10.0);
    let (_, b1y) = p(1.5, 25.8, 10.0);
    fb.line(t0, t0y, t1, t1y, CROWD_RED);
    fb.line(t0, b0y, t1, b1y, CROWD_RED);
    fb.line(t0, t0y, t0, b0y, CROWD_RED);
    fb.line(t1, t1y, t1, b1y, CROWD_RED);
    // Shot clock above the backboard.
    let (cx, cy) = p(1.5, HOOP_Y, 14.0);
    fb.line(cx, cy, cx, cy - 3, STEEL);
    fb.rect(cx - 9, cy - 13, 18, 10, GRAY);
    fb.rect(cx - 8, cy - 12, 16, 8, BLACK);
    fb.text(
        cx - 7,
        cy - 11,
        &format!("{:02}", shot_clock.ceil() as u32),
        LED,
        2,
    );
}

fn draw_rim_and_net(fb: &mut Framebuffer, cam: f32, mirror: bool) {
    let p = |x: f32, y: f32, z: f32| hoop_pt(cam, mirror, x, y, z);
    const N: usize = 16;
    let ring = |r: f32, z: f32, i: usize| {
        let a = i as f32 * (core::f32::consts::TAU / N as f32);
        p(5.25 + r * a.cos(), HOOP_Y + r * a.sin(), z)
    };
    // Net: strands from the rim down to a smaller ring.
    for i in 0..N {
        let (x0, y0) = ring(0.75, RIM_Z, i);
        let (x1, y1) = ring(0.38, RIM_Z - 2.4, i);
        fb.line(x0, y0, x1, y1, if i % 2 == 0 { WHITE } else { GRAY });
    }
    for i in 0..N {
        let (x0, y0) = ring(0.56, RIM_Z - 1.2, i);
        let (x1, y1) = ring(0.56, RIM_Z - 1.2, (i + 1) % N);
        fb.line(x0, y0, x1, y1, GRAY);
    }
    for i in 0..N {
        let (x0, y0) = ring(0.75, RIM_Z, i);
        let (x1, y1) = ring(0.75, RIM_Z, (i + 1) % N);
        fb.line(x0, y0, x1, y1, BALL);
    }
}

// ---------------------------------------------------------------- people and ball

enum Drawable {
    Player(usize),
    Referee,
    Ball,
}

fn draw_entities(g: &Game, fb: &mut Framebuffer, cam: f32) {
    let ref_x = g.ball.x.clamp(8.0, COURT_L - 8.0);
    let ref_y = COURT_W + 2.0;
    let mut order: Vec<(f32, Drawable)> = Vec::with_capacity(7);
    for (i, p) in g.players.iter().enumerate() {
        order.push((p.y, Drawable::Player(i)));
    }
    order.push((ref_y, Drawable::Referee));
    let ball_key = match g.ball.state {
        BallState::Held(h) => g.players[h].y + 0.01,
        _ => g.ball.y,
    };
    order.push((ball_key, Drawable::Ball));
    order.sort_by(|a, b| a.0.total_cmp(&b.0));

    // Shadows first so nobody is painted over by someone else's shadow.
    for p in &g.players {
        let (sx, sy) = project(cam, p.x, p.y, 0.0);
        fb.rect(sx - 6, sy - 1, 13, 3, SHADOW);
    }
    let (rx, ry) = project(cam, ref_x, ref_y, 0.0);
    fb.rect(rx - 6, ry - 1, 13, 3, SHADOW);
    let (bx, by) = project(cam, g.ball.x, g.ball.y, 0.0);
    fb.rect(bx - 2, by, 5, 2, SHADOW);

    for (_, d) in order {
        match d {
            Drawable::Player(i) => draw_player(g, fb, cam, i),
            Drawable::Referee => draw_person(
                fb,
                rx,
                ry,
                1,
                &REF_LOOK,
                Pose {
                    frame: 0,
                    moving: false,
                    jump: false,
                    handling: false,
                },
                false,
            ),
            Drawable::Ball => draw_ball(g, fb, cam),
        }
    }
}

fn draw_ball(g: &Game, fb: &mut Framebuffer, cam: f32) {
    let (sx, sy) = project(cam, g.ball.x, g.ball.y, g.ball.z);
    let sy = sy - 2;
    fb.rect(sx - 1, sy - 2, 3, 5, BALL);
    fb.rect(sx - 2, sy - 1, 5, 3, BALL);
    fb.rect(sx - 2, sy, 5, 1, BALL_SEAM);
    fb.rect(sx, sy - 2, 1, 5, BALL_SEAM);
}

#[derive(Clone, Copy)]
struct Pose {
    frame: i32,
    moving: bool,
    jump: bool,
    handling: bool,
}

fn draw_player(g: &Game, fb: &mut Framebuffer, cam: f32, i: usize) {
    let p = &g.players[i];
    let jump_h = if p.jump > 0.0 {
        let t = 1.0 - p.jump / 0.5;
        (4.0 * t * (1.0 - t) * 2.4 * Z_PX) as i32
    } else {
        0
    };
    let (sx, sy) = project(cam, p.x, p.y, 0.0);
    let pose = Pose {
        frame: ((p.anim * 0.45) as i32) & 3,
        moving: (p.vx * p.vx + p.vy * p.vy) > 4.0,
        jump: p.jump > 0.0,
        handling: matches!(g.ball.state, BallState::Held(h) if h == i),
    };
    let controlled = i == g.controlled && g.phase == Phase::Playing;
    draw_person(
        fb,
        sx,
        sy - jump_h,
        p.facing as i32,
        &TEAM_LOOKS[i],
        pose,
        controlled,
    );
    if controlled && (g.ticks / 20) % 2 == 0 {
        let top = sy - jump_h - 41;
        fb.rect(sx - 3, top, 7, 1, WHITE);
        fb.rect(sx - 2, top + 1, 5, 1, WHITE);
        fb.rect(sx - 1, top + 2, 3, 1, WHITE);
        fb.px(sx, top + 3, WHITE);
    }
}

/// Draw a standing/running person about 28 px tall, feet at (`sx`, `sy`).
/// The player the human controls wears black shoes, like in the original.
fn draw_person(
    fb: &mut Framebuffer,
    sx: i32,
    sy: i32,
    f: i32,
    look: &Look,
    pose: Pose,
    black_shoes: bool,
) {
    // Mirror-aware rectangle: dx0..=dx1 are offsets toward the facing direction.
    let mut r = |dx0: i32, dx1: i32, top: i32, bot: i32, c: u8| {
        let (a, b) = if f >= 0 {
            (sx + dx0, sx + dx1)
        } else {
            (sx - dx1, sx - dx0)
        };
        fb.rect(a, sy + top, b - a + 1, bot - top + 1, c);
    };

    // Stride per walk-cycle frame: (back leg offset, front leg offset, back lift, front lift).
    let (back_s, front_s, back_lift, front_lift) = if pose.jump {
        (-1, 1, 2, 2)
    } else if !pose.moving {
        (-1, 1, 0, 0)
    } else {
        match pose.frame {
            0 => (-3, 3, 0, 0),
            1 => (-1, 1, 0, 3),
            2 => (3, -3, 0, 0),
            _ => (1, -1, 3, 0),
        }
    };
    let shoe = if black_shoes { BLACK } else { WHITE };
    for (hip, stride, lift) in [(-2, back_s, back_lift), (1, front_s, front_lift)] {
        let top_off = stride / 2;
        r(hip + top_off, hip + top_off + 1, -5, -4 + lift, look.skin);
        if lift < 3 {
            r(hip + stride, hip + stride + 1, -3 + lift, -2, look.sock);
            r(hip + stride, hip + stride + 2, -1, 0, shoe);
        }
    }
    // Shorts.
    r(-3, 3, -11, -6, look.shorts);
    r(
        3,
        3,
        -11,
        -6,
        if look.stripes { look.shorts } else { JERSEY },
    );
    // Torso.
    if look.stripes {
        for row in -22..=-12 {
            r(-3, 3, row, row, if row % 2 == 0 { BLACK } else { WHITE });
        }
    } else {
        r(-3, 3, -22, -12, look.top);
        r(-3, -3, -22, -12, look.trim);
        r(3, 3, -22, -12, look.trim);
        r(-1, 1, -22, -22, look.skin);
    }
    // Neck and head.
    r(-1, 1, -23, -23, look.skin);
    r(-2, 2, -28, -24, look.skin);
    r(-2, 2, -28, -27, look.hair);
    r(-2, -1, -26, -25, look.hair);
    r(1, 1, -26, -26, BLACK);
    // Arms.
    let sleeve = if look.stripes { WHITE } else { look.top };
    if pose.jump {
        r(2, 3, -33, -22, look.skin);
        r(-5, -4, -27, -22, look.skin);
        r(2, 3, -22, -21, sleeve);
    } else if pose.handling {
        r(4, 8, -17, -16, look.skin);
        r(4, 5, -22, -18, look.skin);
        r(-5, -4, -22, -14, look.skin);
        r(4, 5, -22, -21, sleeve);
        r(-5, -4, -22, -21, sleeve);
    } else {
        let swing = if pose.moving {
            [2, 0, -2, 0][pose.frame as usize]
        } else {
            0
        };
        r(4 + swing / 2, 5 + swing / 2, -22, -14, look.skin);
        r(-5 - swing / 2, -4 - swing / 2, -22, -14, look.skin);
        r(4, 5, -22, -21, sleeve);
        r(-5, -4, -22, -21, sleeve);
    }
    // Jersey number (drawn upright regardless of facing).
    if look.number > 0 {
        let s = format!("{}", look.number);
        let w = text_width(&s, 1);
        fb.text(sx - w / 2, sy - 20, &s, look.trim, 1);
    }
}

// ---------------------------------------------------------------- HUD

fn draw_bottom_strip(fb: &mut Framebuffer) {
    fb.rect(0, 184, SCREEN_W as i32, 16, APRON);
    fb.rect(0, 184, SCREEN_W as i32, 1, DARK);
    fb.text_centered(160, 190, "ARROWS MOVE  X SHOOT  Z PASS  C SWITCH", GRAY, 1);
}

fn draw_scoreboard(g: &Game, fb: &mut Framebuffer) {
    // Framed panel like the stadium scoreboard of the originals.
    fb.rect(68, 1, 184, 38, WHITE);
    fb.rect(69, 2, 182, 36, GRAY);
    fb.rect(71, 4, 178, 32, BLACK);

    fb.text(78, 7, "HOME", WHITE, 1);
    fb.text_centered(160, 7, "PER. 1", WHITE, 1);
    fb.text(249 - 8 - text_width("SHOT", 1), 7, "SHOT", WHITE, 1);

    fb.dot_text(78, 16, &format!("{:03}", g.score));
    let secs = g.clock.ceil() as u32;
    let clock = format!("{}:{:02}", secs / 60, secs % 60);
    fb.dot_text(160 - (clock.len() as i32 * 12 - 3) / 2, 16, &clock);
    fb.dot_text(
        249 - 8 - 21,
        16,
        &format!("{:02}", g.shot_clock.ceil() as u32),
    );
}

fn draw_overlays(g: &Game, fb: &mut Framebuffer) {
    if g.phase == Phase::Final {
        fb.rect(70, 104, 180, 44, BLACK);
        fb.text_centered(160, 110, "FINAL", LED, 4);
        fb.text_centered(160, 134, "PRESS ENTER", WHITE, 1);
        return;
    }
    if let Some((msg, _)) = g.message {
        let w = text_width(msg, 2) + 8;
        fb.rect(160 - w / 2, 44, w, 14, BLACK);
        fb.text_centered(160, 47, msg, WHITE, 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{PASS, RIGHT, SHOOT};

    #[test]
    fn renders_every_state_inside_the_palette() {
        let mut g = Game::new(3);
        let mut fb = Framebuffer::new();
        let script = [RIGHT, RIGHT, PASS, 0, 0, SHOOT, 0, 0];
        for t in 0..1500usize {
            g.step(script[(t / 20) % script.len()]);
            if t % 7 == 0 {
                draw(&g, &mut fb);
                assert!(fb.idx.iter().all(|&c| (c as usize) < PALETTE.len()));
            }
        }
        // Not blank, and the alpha channel is opaque.
        assert!(fb.idx.iter().any(|&c| c != APRON));
        assert!(fb.rgba.chunks(4).all(|p| p[3] == 255));
    }

    #[test]
    fn camera_stays_within_its_limits() {
        let mut g = Game::new(1);
        let mut fb = Framebuffer::new();
        for _ in 0..600 {
            g.step(RIGHT);
            draw(&g, &mut fb);
            assert!((CAM_MIN..=CAM_MAX).contains(&fb.cam_x));
        }
    }
}
