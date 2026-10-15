//! Software renderer: draws one frame of the game into a 320x200 paletted
//! framebuffer, then expands it to RGBA for the host to blit.
//!
//! The look follows the late-80s/early-90s originals: a raised sideline view that scrolls to
//! follow the ball (half a court on screen), strong perspective toward the near sideline, multi-tone
//! hardwood planks, a painted lane, a 3/4-view hoop with a shot clock on the stanchion, benches and
//! a scorer's table behind the far sideline, and an amber dot-matrix scoreboard.

use crate::font::{glyph, text_width, GLYPH_W};
use crate::game::{BallState, Game, Phase, View, HOOP_X, HOOP_Y, RIM_Z};
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
    [214, 160, 86],
    [204, 148, 76],
    [186, 130, 64],
    [222, 170, 98],
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
const COURT_TOP: i32 = 70;
const COURT_PX_H: i32 = 114;
const PX_PER_FT_Y: f32 = COURT_PX_H as f32 / COURT_W;
const Z_PX: f32 = 5.2; // screen pixels per foot of height
const CAM_MIN: f32 = 28.0;
const CAM_MAX: f32 = COURT_L - 28.0;
const FAR_SCALE: f32 = 4.2; // pixels per foot along x at the far sideline
const NEAR_SCALE: f32 = 5.9;

// End-on view (looking at the right-hand basket from midcourt).
const END_BASE_Y: f32 = 56.0; // screen row of the baseline
const END_KY: f32 = 4.4; // screen rows per foot of depth
const END_ZPX: f32 = 4.0; // screen pixels per foot of height
const END_W_FAR: f32 = 5.0; // pixels per foot of court width at the baseline
const END_W_SLOPE: f32 = 0.07; // ...growing by this much per foot toward the camera // ... and at the near sideline

pub struct Framebuffer {
    pub idx: Vec<u8>,
    pub rgba: Vec<u8>,
    cam_x: f32,
    cam_ready: bool,
    last_view: Option<View>,
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
            last_view: None,
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

/// End-on projection. Depth `d` is the distance in front of the right-hand baseline; court
/// width runs across the screen with the near sideline (y = 50) on the left.
pub fn project_end(x: f32, y: f32, z: f32) -> (i32, i32) {
    let d = COURT_L - x;
    let sx = 160.0 - (y - COURT_W / 2.0) * end_w(d);
    let sy = END_BASE_Y + d * END_KY - z * END_ZPX;
    (sx.round() as i32, sy.round() as i32)
}

fn end_w(d: f32) -> f32 {
    END_W_FAR + END_W_SLOPE * d
}

/// The active camera: the view plus (for the side view) the scroll position.
#[derive(Clone, Copy)]
struct Cam {
    view: View,
    x: f32,
}

impl Cam {
    fn project(self, x: f32, y: f32, z: f32) -> (i32, i32) {
        match self.view {
            View::Side => project(self.x, x, y, z),
            View::End => project_end(x, y, z),
        }
    }

    fn z_px(self) -> f32 {
        match self.view {
            View::Side => Z_PX,
            View::End => END_ZPX,
        }
    }
}

pub fn draw(g: &Game, fb: &mut Framebuffer) {
    if fb.last_view != Some(g.view) {
        fb.last_view = Some(g.view); // a camera cut: don't ease across it
        fb.cam_ready = false;
    }
    fb.update_camera(g);
    let cam = Cam {
        view: g.view,
        x: fb.cam_x,
    };
    match g.view {
        View::Side => draw_side_scene(g, fb, cam),
        View::End => draw_end_scene(g, fb, cam),
    }
    draw_bottom_strip(fb);
    draw_scoreboard(g, fb);
    draw_overlays(g, fb);
    fb.expand();
}

fn draw_side_scene(g: &Game, fb: &mut Framebuffer, cam: Cam) {
    fb.rect(0, 0, SCREEN_W as i32, SCREEN_H as i32, APRON);
    draw_crowd(fb, cam.x, g.ticks);
    draw_bench_area(fb, cam.x);
    draw_court(fb, cam.x);
    for mirror in [false, true] {
        draw_hoop_back(fb, cam.x, mirror, g.shot_clock);
    }
    draw_entities(g, fb, cam);
    for mirror in [false, true] {
        draw_rim_and_net(fb, cam.x, mirror);
    }
}

fn draw_end_scene(g: &Game, fb: &mut Framebuffer, cam: Cam) {
    fb.rect(0, 0, SCREEN_W as i32, SCREEN_H as i32, APRON);
    draw_crowd(fb, COURT_L / 2.0, g.ticks);
    fb.rect(72, 0, 176, 36, WALL); // dark wall behind the basket
    fb.rect(0, 48, SCREEN_W as i32, 8, APRON);
    draw_end_staff(fb);
    draw_court_end(fb);
    draw_end_hoop_back(fb, g.shot_clock);
    draw_entities(g, fb, cam);
    draw_end_rim_and_net(fb);
}

fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

// ---------------------------------------------------------------- arena

fn draw_crowd(fb: &mut Framebuffer, cam: f32, ticks: u64) {
    fb.rect(0, 0, SCREEN_W as i32, 48, WALL);
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
    for row in 0..5 {
        let y0 = 2 + row * 7;
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
    // Barrier wall with courtside advertising boards (these scroll with the floor).
    fb.rect(0, 35, SCREEN_W as i32, 1, DARK);
    fb.rect(0, 36, SCREEN_W as i32, 12, SEATS);
    let ad_off = (cam * 4.0) as i32;
    let (ad_base, ad_x) = (ad_off.div_euclid(52), ad_off.rem_euclid(52));
    const ADS: [&str; 4] = ["NTBA", "HOOPS", "SPORTS", "OPEN"];
    for k in 0..=(SCREEN_W as i32 / 52 + 1) {
        let idx = ad_base + k;
        let x = k * 52 - ad_x;
        let white = idx.rem_euclid(2) == 0;
        fb.rect(x + 2, 38, 46, 8, if white { WHITE } else { PAINT });
        let label = ADS[idx.rem_euclid(4) as usize];
        fb.text(
            x + 25 - text_width(label, 1) / 2,
            40,
            label,
            if white { PAINT_DARK } else { WHITE },
            1,
        );
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
    long_pants: bool,
}

const fn team_look(skin: u8, hair: u8, number: u8) -> Look {
    Look {
        skin,
        hair,
        top: JERSEY,
        trim: TRIM,
        shorts: TRIM,
        sock: WHITE,
        number,
        stripes: false,
        long_pants: false,
    }
}

const TEAM_LOOKS: [Look; 5] = [
    team_look(SKIN_MID, BLACK, 4),
    team_look(SKIN_LIGHT, HAIR_BLOND, 12),
    team_look(SKIN_DARK, BLACK, 23),
    team_look(SKIN_MID, BLACK, 32),
    team_look(SKIN_DARK, BLACK, 45),
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
    long_pants: true,
};
const COACH_LOOK: Look = Look {
    skin: SKIN_LIGHT,
    hair: GRAY,
    top: SUIT,
    trim: WHITE,
    shorts: SUIT,
    sock: SUIT,
    number: 0,
    stripes: false,
    long_pants: true,
};

/// A seated person behind the far sideline (bench or table). Feet row is `sy`; about 24 px tall.
fn draw_seated(fb: &mut Framebuffer, sx: i32, sy: i32, jacket: u8, skin: u8, hair: u8, suit: bool) {
    fb.rect(sx - 5, sy - 19, 11, 14, GRAY); // folding chair back
    fb.rect(sx - 4, sy - 17, 9, 10, jacket);
    if suit {
        fb.rect(sx - 1, sy - 17, 3, 5, WHITE);
        fb.rect(sx, sy - 16, 1, 4, CROWD_RED);
    } else {
        fb.rect(sx, sy - 17, 1, 10, WHITE); // zip stripe
    }
    fb.rect(sx - 2, sy - 24, 5, 6, skin);
    fb.rect(sx - 2, sy - 24, 5, 2, hair);
    let legs = if suit { SUIT } else { jacket };
    fb.rect(sx - 4, sy - 7, 4, 5, legs);
    fb.rect(sx + 1, sy - 7, 4, 5, legs);
    fb.rect(sx - 4, sy - 2, 4, 2, WHITE);
    fb.rect(sx + 1, sy - 2, 4, 2, WHITE);
}

const BENCH_FEET: i32 = 68;

fn draw_bench_area(fb: &mut Framebuffer, cam: f32) {
    fb.rect(0, 48, SCREEN_W as i32, 22, APRON);
    // Benches: home on the left in gold warm-ups, visitors on the right in green.
    for (k, x) in [8.0_f32, 11.5, 15.0, 18.5, 22.0].iter().enumerate() {
        let (sx, _) = project(cam, *x, -1.5, 0.0);
        let look = &TEAM_LOOKS[k];
        draw_seated(fb, sx, BENCH_FEET, JERSEY, look.skin, look.hair, false);
    }
    for (k, x) in [66.0_f32, 69.5, 73.0, 76.5, 80.0].iter().enumerate() {
        let (sx, _) = project(cam, *x, -1.5, 0.0);
        let skin = [SKIN_LIGHT, SKIN_DARK, SKIN_MID, SKIN_DARK, SKIN_LIGHT][k];
        draw_seated(fb, sx, BENCH_FEET, CROWD_GREEN, skin, BLACK, false);
    }
    for x in [25.5_f32, 83.5, 87.0] {
        let (sx, _) = project(cam, x, -1.5, 0.0);
        draw_seated(fb, sx, BENCH_FEET, SUIT, SKIN_LIGHT, GRAY, true);
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
        draw_seated(fb, sx, 62, jacket, skin, BLACK, k != 1);
    }
    fb.rect(x0, 55, x1 - x0, 2, WHITE);
    fb.rect(x0, 57, x1 - x0, 11, PAINT);
    let mut x = x0;
    while x < x1 {
        fb.rect(x, 57, 1, 11, PAINT_DARK);
        x += 5;
    }
    let mut x = x0;
    while x < x1 {
        fb.rect(x, 68, 3, 1, PAINT);
        x += 6;
    }
    for mx in [x0 + 8, x0 + 26, x0 + 46] {
        fb.rect(mx, 50, 7, 5, BLACK);
        fb.rect(mx + 1, 51, 5, 3, GRAY);
    }
    fb.rect(x0 + 14, 54, 6, 1, WHITE);
}

// ---------------------------------------------------------------- court

fn near(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() < tol
}

/// True when (`dx`, `dy`) feet from a circle's centre lies on its ring, with a line about one
/// screen pixel wide regardless of the local horizontal/vertical scale.
fn ring(dx: f32, dy: f32, radius: f32, kx: f32, ky: f32) -> bool {
    let d = (dx * dx + dy * dy).sqrt();
    if d < 0.001 {
        return false;
    }
    let (gx, gy) = (kx * dx / d, ky * dy / d);
    let scale = (gx * gx + gy * gy).sqrt();
    (d - radius).abs() < 0.6 / scale
}

/// Colour of the hardwood at a court position (feet), markings included. `s` is the local
/// pixels-per-foot along world x (`kx`) and y (`ky`) so line widths stay about one pixel.
fn floor_colour(wx: f32, wy: f32, kx: f32, ky: f32) -> u8 {
    let mx = wx.min(COURT_L - wx); // everything but the centre line is symmetric
    let ltx = 0.6 / kx; // half line width along x (feet)
    let lty = 0.6 / ky; // half line width along y (feet)
    let (hx, hy) = (mx - 5.25, wy - HOOP_Y);
    let (fx, fy) = (mx - 19.0, wy - HOOP_Y);
    let (cx, cy) = (wx - COURT_L / 2.0, wy - HOOP_Y);
    let centre_d = (cx * cx + cy * cy).sqrt();
    let in_lane = mx < 19.0 && (wy - HOOP_Y).abs() < 8.0;

    let line = wy < 1.6 * lty
        || wy > COURT_W - 1.6 * lty
        || mx < 1.6 * ltx
        || near(wx, COURT_L / 2.0, ltx)
        || ring(cx, cy, 6.0, kx, ky)
        || (mx < 19.0 && (near(wy, 17.0, lty) || near(wy, 33.0, lty)))
        || (near(mx, 19.0, ltx) && in_lane)
        || (ring(fx, fy, 6.0, kx, ky) && (mx >= 19.0 || (wy * 1.4) as i32 & 1 == 0))
        || (ring(hx, hy, 23.75, kx, ky) && hy.abs() < 22.0 && mx > 5.25)
        || (mx < 14.0 && (near(wy, 3.0, lty) || near(wy, COURT_W - 3.0, lty)));
    if line {
        return BLACK;
    }
    // Painted "NTBA" logo in the centre circle (23x5 cells, stretched like floor art).
    const LOGO: [&[u8; 23]; 5] = [
        b"10001011111011110001110",
        b"11001000100010001010001",
        b"10101000100011110011111",
        b"10011000100010001010001",
        b"10001000100011110010001",
    ];
    let (u, v) = ((cx + 5.175) / 0.45, (cy + 1.625) / 0.65);
    if (0.0..23.0).contains(&u) && (0.0..5.0).contains(&v) && LOGO[v as usize][u as usize] == b'1' {
        return WHITE;
    }
    if in_lane || centre_d < 6.0 {
        return PAINT;
    }
    // Hardwood: horizontal planks, each with its own length, offset and a close-ish tone.
    let row = wy as i32;
    let h = hash(row as u32, 7);
    let len = 6.0 + (h % 6) as f32;
    let off = ((h >> 8) % 97) as f32;
    let t = (wx + off) / len;
    let plank = t.floor() as i32;
    if t - t.floor() < 0.25 / len {
        return WOOD_DARK; // butt joint between boards
    }
    match hash(row as u32, plank as u32 ^ 0xABCD) % 8 {
        0 | 1 | 2 | 7 => WOOD_MID,
        3 | 4 => WOOD_LIGHT,
        5 => WOOD_DARK,
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
                fb.px(sx, sy, floor_colour(wx, wy, s, PX_PER_FT_Y));
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

fn draw_court_end(fb: &mut Framebuffer) {
    for sy in END_BASE_Y as i32..184 {
        let d = (sy as f32 - END_BASE_Y) / END_KY;
        let wx = COURT_L - d;
        let w = end_w(d);
        for sx in 0..SCREEN_W as i32 {
            let wy = COURT_W / 2.0 - (sx as f32 - 160.0) / w;
            if (0.0..=COURT_W).contains(&wy) && wx >= 0.0 {
                fb.px(sx, sy, floor_colour(wx, wy, END_KY, w));
            }
        }
    }
}

/// Coaches, photographers and the referee behind the baseline.
fn draw_end_staff(fb: &mut Framebuffer) {
    for (k, y) in [3.0_f32, 9.0, 41.0, 47.0].iter().enumerate() {
        let (sx, sy) = project_end(97.0, *y, 0.0);
        let mut look = COACH_LOOK;
        look.skin = [SKIN_LIGHT, SKIN_DARK, SKIN_MID, SKIN_LIGHT][k];
        draw_person_end(fb, sx, sy, false, &look, STILL, false);
    }
    let (sx, sy) = project_end(95.0, 53.0, 0.0);
    draw_person_end(fb, sx, sy, false, &REF_LOOK, STILL, false);
}

/// Stanchion, backboard (seen face-on) and the shot clock above it. The board is placed
/// relative to the rim, not by true depth, so it stays on screen like in the original.
fn draw_end_hoop_back(fb: &mut Framebuffer, shot_clock: f32) {
    let (rx, ry) = project_end(HOOP_X, HOOP_Y, RIM_Z);
    let (bx, by) = project_end(COURT_L + 3.0, HOOP_Y, 0.0);
    fb.rect(bx - 8, by - 9, 16, 9, PAINT);
    fb.rect(bx - 8, by - 9, 4, 9, PAINT_DARK);
    fb.rect(rx - 1, ry - 4, 3, by - 9 - (ry - 4), STEEL);
    fb.rect(rx - 14, ry - 19, 29, 16, STEEL);
    fb.rect(rx - 13, ry - 18, 27, 14, WHITE);
    fb.rect(rx - 5, ry - 13, 11, 8, CROWD_RED);
    fb.rect(rx - 4, ry - 12, 9, 6, WHITE);
    fb.rect(rx - 11, ry - 31, 23, 11, GRAY);
    fb.rect(rx - 10, ry - 30, 21, 9, BLACK);
    fb.text(
        rx - 6,
        ry - 29,
        &format!("{:02}", shot_clock.ceil() as u32),
        LED,
        2,
    );
}

fn draw_end_rim_and_net(fb: &mut Framebuffer) {
    const N: usize = 16;
    let ring = |r: f32, z: f32, i: usize| {
        let a = i as f32 * (core::f32::consts::TAU / N as f32);
        project_end(HOOP_X + r * a.cos(), HOOP_Y + r * a.sin(), z)
    };
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

const COACH_X: f32 = 28.0;
const COACH_Y: f32 = -0.6;
const STILL: Pose = Pose {
    frame: 0,
    moving: false,
    jump: false,
    handling: false,
};

#[derive(Clone, Copy)]
struct Pose {
    frame: i32,
    moving: bool,
    jump: bool,
    handling: bool,
}

enum Drawable {
    Player(usize),
    Referee,
    Coach,
    Ball,
}

fn draw_entities(g: &Game, fb: &mut Framebuffer, cam: Cam) {
    let side = cam.view == View::Side;
    let ref_x = g.ball.x.clamp(8.0, COURT_L - 8.0);
    let ref_y = COURT_W + 2.0;
    // Painter's order: far to near. Side view: small y is far. End view: large x is far.
    let key = |x: f32, y: f32| if side { y } else { -x };
    let mut order: Vec<(f32, Drawable)> = Vec::with_capacity(7);
    for (i, p) in g.players.iter().enumerate() {
        order.push((key(p.x, p.y), Drawable::Player(i)));
    }
    if side {
        order.push((key(ref_x, ref_y), Drawable::Referee));
        order.push((key(COACH_X, COACH_Y), Drawable::Coach));
    }
    let ball_key = match g.ball.state {
        BallState::Held(h) => key(g.players[h].x, g.players[h].y) + 0.01,
        _ => key(g.ball.x, g.ball.y),
    };
    order.push((ball_key, Drawable::Ball));
    order.sort_by(|a, b| a.0.total_cmp(&b.0));

    // Shadows first so nobody is painted over by someone else's shadow.
    for p in &g.players {
        let (sx, sy) = cam.project(p.x, p.y, 0.0);
        fb.rect(sx - 6, sy - 1, 13, 3, SHADOW);
    }
    let (cxs, cys) = cam.project(COACH_X, COACH_Y, 0.0);
    let (rx, ry) = cam.project(ref_x, ref_y, 0.0);
    if side {
        fb.rect(cxs - 6, cys - 1, 13, 3, SHADOW);
        fb.rect(rx - 6, ry - 1, 13, 3, SHADOW);
    }
    let (bx, by) = cam.project(g.ball.x, g.ball.y, 0.0);
    fb.rect(bx - 2, by, 5, 2, SHADOW);

    for (_, d) in order {
        match d {
            Drawable::Player(i) => draw_player(g, fb, cam, i),
            Drawable::Referee => draw_person(fb, rx, ry, 1, &REF_LOOK, STILL, false),
            Drawable::Coach => draw_person(fb, cxs, cys, 1, &COACH_LOOK, STILL, false),
            Drawable::Ball => draw_ball(g, fb, cam),
        }
    }
}

fn draw_ball(g: &Game, fb: &mut Framebuffer, cam: Cam) {
    let (sx, sy) = match (cam.view, g.ball.state) {
        // Seen from behind, a dribbled ball would hide behind the body: show it at the hand.
        (View::End, BallState::Held(h)) => {
            let p = &g.players[h];
            let (hx, hy) = cam.project(p.x, p.y, 0.0);
            let side = if p.facing >= 0 { 7 } else { -7 };
            (hx + side, hy - (g.ball.z * END_ZPX) as i32)
        }
        _ => cam.project(g.ball.x, g.ball.y, g.ball.z),
    };
    let sy = sy - 2;
    fb.rect(sx - 1, sy - 2, 3, 5, BALL);
    fb.rect(sx - 2, sy - 1, 5, 3, BALL);
    fb.rect(sx - 2, sy, 5, 1, BALL_SEAM);
    fb.rect(sx, sy - 2, 1, 5, BALL_SEAM);
}

fn draw_player(g: &Game, fb: &mut Framebuffer, cam: Cam, i: usize) {
    let p = &g.players[i];
    let jump_h = if p.jump > 0.0 {
        let t = 1.0 - p.jump / 0.5;
        (4.0 * t * (1.0 - t) * 2.4 * cam.z_px()) as i32
    } else {
        0
    };
    let (sx, sy) = cam.project(p.x, p.y, 0.0);
    let pose = Pose {
        frame: ((p.anim * 0.45) as i32) & 3,
        moving: (p.vx * p.vx + p.vy * p.vy) > 4.0,
        jump: p.jump > 0.0,
        handling: matches!(g.ball.state, BallState::Held(h) if h == i),
    };
    let controlled = i == g.controlled && g.phase == Phase::Playing;
    let head = match cam.view {
        View::Side => {
            draw_person(
                fb,
                sx,
                sy - jump_h,
                p.facing as i32,
                &TEAM_LOOKS[i],
                pose,
                controlled,
            );
            46
        }
        View::End => {
            // Moving toward the basket shows the player's back.
            draw_person_end(
                fb,
                sx,
                sy - jump_h,
                p.facing >= 0,
                &TEAM_LOOKS[i],
                pose,
                controlled,
            );
            36
        }
    };
    if controlled && (g.ticks / 20) % 2 == 0 {
        let top = sy - jump_h - head;
        fb.rect(sx - 3, top, 7, 1, WHITE);
        fb.rect(sx - 2, top + 1, 5, 1, WHITE);
        fb.rect(sx - 1, top + 2, 3, 1, WHITE);
        fb.px(sx, top + 3, WHITE);
    }
}

/// A person seen from behind (`back`) or the front, about 27 px tall, feet at (`sx`, `sy`).
/// Used by the end-on camera, where players run toward or away from the viewer.
fn draw_person_end(
    fb: &mut Framebuffer,
    sx: i32,
    sy: i32,
    back: bool,
    look: &Look,
    pose: Pose,
    black_shoes: bool,
) {
    let bob = if pose.moving && pose.frame & 1 == 1 {
        1
    } else {
        0
    };
    let mut r = |dx0: i32, dx1: i32, top: i32, bot: i32, c: u8| {
        let up = if top < -8 { bob } else { 0 };
        fb.rect(sx + dx0, sy + top - up, dx1 - dx0 + 1, bot - top + 1, c);
    };
    let leg = if look.long_pants {
        look.shorts
    } else {
        look.skin
    };
    let shoe = if black_shoes { BLACK } else { WHITE };
    let (left_up, right_up) = if pose.jump {
        (true, true)
    } else if pose.moving {
        match pose.frame {
            1 => (true, false),
            3 => (false, true),
            _ => (false, false),
        }
    } else {
        (false, false)
    };
    for (dx, up) in [(-2, left_up), (1, right_up)] {
        if up {
            r(dx, dx + 1, -7, -4, leg);
            r(dx - 1, dx + 1, -3, -2, shoe);
        } else {
            r(dx, dx + 1, -7, -2, leg);
            if !look.long_pants {
                r(dx, dx + 1, -4, -2, look.sock);
            }
            r(dx - 1, dx + 1, -1, 0, shoe);
        }
    }
    // Shorts with a baggy hem, then the jersey.
    r(-3, 3, -12, -8, look.shorts);
    r(-4, 4, -10, -8, look.shorts);
    if look.stripes {
        for row in -20..=-13 {
            r(-3, 3, row, row, if row % 2 == 0 { BLACK } else { WHITE });
        }
    } else {
        r(-3, 3, -20, -13, look.top);
        if look.long_pants {
            if !back {
                r(-1, 1, -20, -15, WHITE);
                r(0, 0, -19, -15, CROWD_RED);
            }
        } else {
            r(-3, -3, -20, -13, look.trim);
            r(3, 3, -20, -13, look.trim);
        }
    }
    // Neck and head: the back of the head is all hair, the front shows a face.
    r(-1, 1, -21, -21, look.skin);
    r(-2, 2, -26, -22, if back { look.hair } else { look.skin });
    if back {
        r(-2, 2, -23, -22, look.skin);
    } else {
        r(-2, 2, -26, -25, look.hair);
        r(-1, -1, -24, -24, BLACK);
        r(1, 1, -24, -24, BLACK);
    }
    // Arms.
    let sleeve = if look.stripes || look.long_pants {
        look.top
    } else {
        look.skin
    };
    if pose.jump {
        r(-4, -3, -31, -20, look.skin);
        r(3, 4, -31, -20, look.skin);
    } else {
        let swing = if pose.moving && pose.frame & 1 == 1 {
            1
        } else {
            0
        };
        r(-5, -4, -20, -18, sleeve);
        r(4, 5, -20, -18, sleeve);
        r(-5, -4, -17, -14 - swing, look.skin);
        r(4, 5, -17, -14 + swing, look.skin);
        if pose.handling {
            r(6, 7, -16, -14, look.skin);
        }
    }
    if look.number > 0 {
        let s = format!("{}", look.number);
        let w = text_width(&s, 1);
        fb.text(sx - w / 2, sy - 19 - bob, &s, look.trim, 1);
    }
}

/// Draw a standing/running person about 34 px tall, feet at (`sx`, `sy`).
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
    // Body bobs one pixel on the passing frames of the run cycle.
    let bob = if pose.moving && (pose.frame == 1 || pose.frame == 3) {
        1
    } else {
        0
    };
    // Mirror-aware rectangle: dx0..=dx1 are offsets toward the facing direction.
    let mut r = |dx0: i32, dx1: i32, top: i32, bot: i32, c: u8| {
        let up = if top < -9 { bob } else { 0 };
        let (a, b) = if f >= 0 {
            (sx + dx0, sx + dx1)
        } else {
            (sx - dx1, sx - dx0)
        };
        fb.rect(a, sy + top - up, b - a + 1, bot - top + 1, c);
    };

    // Run cycle: (back stride, front stride, back leg lifted, front leg lifted).
    let (back_s, front_s, back_up, front_up) = if pose.jump {
        (-1, 1, true, true)
    } else if !pose.moving {
        (-1, 1, false, false)
    } else {
        match pose.frame {
            0 => (-4, 4, false, false),
            1 => (-1, 1, false, true),
            2 => (4, -4, false, false),
            _ => (1, -1, true, false),
        }
    };
    let leg = if look.long_pants {
        look.shorts
    } else {
        look.skin
    };
    let shoe = if black_shoes { BLACK } else { WHITE };
    for (hip, stride, up) in [(-2, back_s, back_up), (1, front_s, front_up)] {
        if up {
            // Knee driven up, shin tucked behind.
            r(hip + 1, hip + 2, -9, -6, leg);
            r(hip - 1, hip, -6, -4, leg);
            r(hip - 1, hip + 1, -3, -2, shoe);
        } else {
            r(hip + stride / 2, hip + stride / 2 + 1, -9, -6, leg);
            r(
                hip + stride,
                hip + stride + 1,
                -5,
                -2,
                if look.long_pants { leg } else { look.sock },
            );
            r(hip + stride, hip + stride + 2, -1, 0, shoe);
        }
    }
    // Shorts (baggy flare at the hem).
    r(-3, 3, -16, -10, look.shorts);
    r(-4, 4, -12, -10, look.shorts);
    if !look.stripes && !look.long_pants {
        r(3, 3, -16, -10, JERSEY);
    }
    // Torso: sleeveless jersey with trim down the sides, or stripes / suit jacket.
    if look.stripes {
        for row in -28..=-17 {
            r(-3, 3, row, row, if row % 2 == 0 { BLACK } else { WHITE });
        }
    } else {
        r(-3, 3, -28, -17, look.top);
        if !look.long_pants {
            r(-3, -3, -28, -17, look.trim);
            r(3, 3, -28, -17, look.trim);
            r(-1, 1, -28, -27, look.skin);
        } else {
            r(-1, 1, -28, -22, WHITE);
            r(0, 0, -27, -22, CROWD_RED);
        }
    }
    // Neck and head.
    r(-1, 1, -29, -29, look.skin);
    r(-2, 2, -34, -30, look.skin);
    r(-2, 2, -34, -33, look.hair);
    r(-2, -1, -32, -31, look.hair);
    r(1, 1, -32, -32, BLACK);
    // Arms.
    let sleeve = if look.stripes || look.long_pants {
        look.top
    } else {
        look.skin
    };
    let sw = if pose.moving {
        [3, 0, -3, 0][pose.frame as usize]
    } else {
        0
    };
    if pose.jump {
        r(2, 3, -41, -28, look.skin);
        r(-5, -4, -35, -28, look.skin);
    } else if pose.handling {
        r(4, 5, -28, -22, look.skin);
        r(4, 8, -21, -20, look.skin);
        r(-5, -4, -28, -17, look.skin);
    } else {
        r(4, 5, -28, -23, sleeve);
        r(4 + sw, 5 + sw, -22, -17, look.skin);
        r(-5, -4, -28, -23, sleeve);
        r(-5 - sw, -4 - sw, -22, -17, look.skin);
    }
    // Jersey number (drawn upright regardless of facing).
    if look.number > 0 {
        let s = format!("{}", look.number);
        let w = text_width(&s, 1);
        fb.text(sx - w / 2, sy - 25 - bob, &s, look.trim, 1);
    }
}

// ---------------------------------------------------------------- HUD

fn draw_bottom_strip(fb: &mut Framebuffer) {
    fb.rect(0, 184, SCREEN_W as i32, 16, APRON);
    fb.rect(0, 184, SCREEN_W as i32, 1, DARK);
    fb.text_centered(160, 190, "ARROWS MOVE  X SHOOT  Z PASS  C SWITCH", GRAY, 1);
}

fn draw_scoreboard(g: &Game, fb: &mut Framebuffer) {
    // Compact framed panel in the corner, like the later EA basketball games.
    fb.rect(2, 2, 136, 30, WHITE);
    fb.rect(3, 3, 134, 28, GRAY);
    fb.rect(4, 4, 132, 26, BLACK);

    fb.text(10, 6, "TIME", WHITE, 1);
    fb.text(66, 6, "HOME", WHITE, 1);
    fb.text(108, 6, "SHOT", WHITE, 1);

    let secs = g.clock.ceil() as u32;
    fb.dot_text(10, 13, &format!("{}:{:02}", secs / 60, secs % 60));
    fb.dot_text(66, 13, &format!("{:03}", g.score));
    fb.dot_text(108, 13, &format!("{:02}", g.shot_clock.ceil() as u32));
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
        fb.rect(214 - w / 2, 8, w, 14, BLACK);
        fb.text_centered(214, 11, msg, WHITE, 2);
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

    #[test]
    fn end_on_view_renders_and_cuts_back() {
        use crate::game::View;
        let mut g = Game::new(5);
        let mut fb = Framebuffer::new();
        draw(&g, &mut fb);
        let side_frame = fb.idx.clone();
        g.players[0].x = crate::game::END_VIEW_ENTER_X + 4.0;
        g.step(0);
        assert_eq!(g.view, View::End);
        for t in 0..120 {
            g.step(if t < 60 { RIGHT } else { SHOOT });
            draw(&g, &mut fb);
            assert!(fb.idx.iter().all(|&c| (c as usize) < PALETTE.len()));
        }
        assert_ne!(
            fb.idx, side_frame,
            "the end-on view looks different from the side view"
        );
        g.players[0].x = 20.0;
        g.ball.state = crate::game::BallState::Held(0);
        g.step(0);
        assert_eq!(g.view, View::Side);
        draw(&g, &mut fb);
    }
}
