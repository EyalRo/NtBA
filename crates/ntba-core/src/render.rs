//! Software renderer: draws one frame of the game into a 320x200 paletted framebuffer (VGA mode
//! 13h style, see [`crate::palette`]), then expands it to RGBA for the host to blit.
//!
//! The look follows the late-80s/early-90s originals: a raised sideline camera that scrolls with
//! the ball, an end-on half-court camera (see [`crate::game::View`]), dithered hardwood lighting,
//! shaded outlined sprites, a detailed crowd, benches and scorer's table, and an amber dot-matrix
//! scoreboard.

use crate::font::{glyph, text_width, GLYPH_W};
use crate::game::{
    team_of, BallState, Basket, Game, Phase, View, HOOP_X, HOOP_Y, PLAYERS, RIM_Z, TEAM_SIZE,
};
use crate::palette::*;
use crate::people::{end_person, seated_person, side_person, Look, Pose, STILL};
use crate::sprite::Sprite;
use crate::{COURT_L, COURT_W, SCREEN_H, SCREEN_W};

// Screen layout.
const COURT_TOP: i32 = 70;
const COURT_PX_H: i32 = 114;
const PX_PER_FT_Y: f32 = COURT_PX_H as f32 / COURT_W;
const Z_PX: f32 = 5.2; // screen pixels per foot of height
const CAM_MIN: f32 = 28.0;
const CAM_MAX: f32 = COURT_L - 28.0;
const FAR_SCALE: f32 = 4.2; // pixels per foot along x at the far sideline
const NEAR_SCALE: f32 = 5.9; // ... and at the near sideline
const BOTTOM_STRIP: i32 = 184;

// End-on view (looking at the right-hand basket from midcourt).
const END_BASE_Y: f32 = 56.0; // screen row of the baseline
const END_KY: f32 = 4.4; // screen rows per foot of depth
const END_ZPX: f32 = 4.0; // screen pixels per foot of height
const END_W_FAR: f32 = 5.0; // pixels per foot of court width at the baseline
const END_W_SLOPE: f32 = 0.07; // ...growing by this much per foot toward the camera

const BAYER: [[f32; 4]; 4] = [
    [0.0, 8.0, 2.0, 10.0],
    [12.0, 4.0, 14.0, 6.0],
    [3.0, 11.0, 1.0, 9.0],
    [15.0, 7.0, 13.0, 5.0],
];

/// Ordered-dither threshold in (-0.5, 0.5) for screen pixel (`x`, `y`).
fn bayer(x: i32, y: i32) -> f32 {
    BAYER[(y & 3) as usize][(x & 3) as usize] / 16.0 - 0.47
}

pub struct Framebuffer {
    pub idx: Vec<u8>,
    pub rgba: Vec<u8>,
    lut: Vec<[u8; 3]>,
    /// Colour-blind-safe team colours: away wears blue instead of green (see [`team_look`]).
    pub colourblind: bool,
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
            lut: PALETTE
                .iter()
                .map(|c| [vga(c[0]), vga(c[1]), vga(c[2])])
                .collect(),
            colourblind: false,
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

    fn at(&self, x: i32, y: i32) -> u8 {
        if x >= 0 && y >= 0 && (x as usize) < SCREEN_W && (y as usize) < SCREEN_H {
            self.idx[y as usize * SCREEN_W + x as usize]
        } else {
            0
        }
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u8) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.px(xx, yy, c);
            }
        }
    }

    /// Raised/sunken frame: light on top-left, dark on bottom-right.
    fn bevel(&mut self, x: i32, y: i32, w: i32, h: i32, light: u8, dark: u8) {
        self.rect(x, y, w, 1, light);
        self.rect(x, y, 1, h, light);
        self.rect(x, y + h - 1, w, 1, dark);
        self.rect(x + w - 1, y, 1, h, dark);
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

    /// Fill the triangle with corners `v` (pixel centres inside it).
    fn triangle(&mut self, v: [(f32, f32); 3], c: u8) {
        let x0 = v.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor() as i32;
        let x1 = v.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil() as i32;
        let y0 = v.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor() as i32;
        let y1 = v.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil() as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let edge = |a: (f32, f32), b: (f32, f32)| {
                    (b.0 - a.0) * (py - a.1) - (b.1 - a.1) * (px - a.0)
                };
                let e = [edge(v[0], v[1]), edge(v[1], v[2]), edge(v[2], v[0])];
                if e.iter().all(|&k| k >= 0.0) || e.iter().all(|&k| k <= 0.0) {
                    self.px(x, y, c);
                }
            }
        }
    }

    pub fn text(&mut self, x: i32, y: i32, s: &str, c: u8, scale: i32) {
        let mut cx = x;
        for ch in s.chars() {
            for (row, bits) in glyph(ch).iter().enumerate() {
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
            for (row, bits) in glyph(ch).iter().enumerate() {
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

    fn sprite(&mut self, s: &Sprite, x: i32, y: i32) {
        s.blit(x, y, |px, py, c| self.px(px, py, c));
    }

    /// Soft floor shadow: darken the hardwood/paint under an ellipse; the rim is dithered.
    fn shadow(&mut self, cx: i32, cy: i32, rx: i32, ry: i32) {
        for y in -ry..=ry {
            for x in -rx..=rx {
                let d = (x * x) as f32 / (rx * rx) as f32 + (y * y) as f32 / (ry * ry) as f32;
                if d > 1.0 || (d > 0.55 && (x + y) & 1 == 0) {
                    continue;
                }
                let c = self.at(cx + x, cy + y);
                let dark = match c {
                    78..=83 => WOOD[(c as usize - 78).saturating_sub(2)],
                    84..=86 => PAINT[0],
                    _ => c,
                };
                self.px(cx + x, cy + y, dark);
            }
        }
    }

    fn expand(&mut self) {
        for (i, &c) in self.idx.iter().enumerate() {
            let rgb = self.lut[c as usize % self.lut.len()];
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

// ---------------------------------------------------------------- projection

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

fn end_w(d: f32) -> f32 {
    END_W_FAR + END_W_SLOPE * d
}

/// End-on projection. Depth `d` is the distance in front of the right-hand baseline; court
/// width runs across the screen with the near sideline (y = 50) on the left.
pub fn project_end(x: f32, y: f32, z: f32) -> (i32, i32) {
    let d = COURT_L - x;
    let sx = 160.0 + (y - COURT_W / 2.0) * end_w(d);
    let sy = END_BASE_Y + d * END_KY - z * END_ZPX;
    (sx.round() as i32, sy.round() as i32)
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
            View::End(Basket::Right) => project_end(x, y, z),
            View::End(Basket::Left) => project_end(COURT_L - x, COURT_W - y, z),
        }
    }

    fn z_px(self) -> f32 {
        match self.view {
            View::Side => Z_PX,
            View::End(_) => END_ZPX,
        }
    }

    /// Height in pixels of a standing player sprite, feet to top of head.
    fn body_h(self) -> i32 {
        match self.view {
            View::Side => 47,
            View::End(_) => 38,
        }
    }
}

// ---------------------------------------------------------------- frame

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
        View::End(_) => draw_end_scene(g, fb, cam),
    }
    draw_offscreen_markers(g, fb, cam);
    draw_bottom_strip(fb);
    draw_scoreboard(g, fb);
    draw_overlays(g, fb);
    draw_dev_badge(fb);
    fb.expand();
}

fn draw_side_scene(g: &Game, fb: &mut Framebuffer, cam: Cam) {
    fb.rect(0, 0, SCREEN_W as i32, SCREEN_H as i32, APRON[1]);
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
    fb.rect(0, 0, SCREEN_W as i32, SCREEN_H as i32, APRON[1]);
    draw_crowd(fb, COURT_L / 2.0, g.ticks);
    // Dark wall behind the basket, lit from the sides.
    for y in 0..36 {
        for x in 72_i32..248 {
            let edge = ((x - 160).abs() as f32 / 88.0).powi(2);
            let v = (y as f32 / 36.0 * 1.2 + edge * 1.5 + bayer(x, y)).floor() as i32;
            fb.px(x, y, NAVY[v.clamp(0, 2) as usize]);
        }
    }
    fb.rect(0, 48, SCREEN_W as i32, 8, APRON[1]);
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

const SHIRTS: [Ramp; 8] = [RED, GREEN, BLUE, PINK, YELLOW, GOLD, CLOTH, PURPLE];

/// One spectator. `scale` grows for the rows nearer the floor; `dim` darkens the far rows.
fn spectator(h: u32, scale: f32, dim: f32, cheer: bool) -> Sprite {
    let mut s = Sprite::new(18, 20, 9, 14);
    let skin = SKINS[((h >> 5) % 3) as usize];
    let shirt = SHIRTS[(h % 8) as usize];
    let hair = HAIRS[((h >> 9) % 3) as usize];
    if cheer {
        s.capsule(
            2.6 * scale,
            1.0,
            4.0 * scale,
            -6.0 * scale,
            1.0 * scale,
            dim,
            &skin,
        );
    }
    s.ellipse(0.0, 5.0 * scale, 3.6 * scale, 3.4 * scale, dim, &shirt);
    s.ellipse(0.0, 0.3 * scale, 2.2 * scale, 2.5 * scale, dim, &skin);
    s.ellipse(0.0, -0.8 * scale, 2.3 * scale, 1.7 * scale, dim, &hair);
    s.outline();
    s
}

fn draw_crowd(fb: &mut Framebuffer, cam: f32, ticks: u64) {
    // Dark stands, a touch lighter toward the bottom.
    for y in 0..48 {
        for x in 0..SCREEN_W as i32 {
            let v = (y as f32 / 36.0 * 1.4 + bayer(x, y)).floor() as i32;
            fb.px(x, y, NAVY[v.clamp(0, 2) as usize]);
        }
    }
    for row in 0..5 {
        let r = row as f32;
        let scale = 0.8 + r * 0.1;
        let spacing = 5.0 + r * 0.5;
        let dim = -0.55 + r * 0.14;
        let y = 5 + (row as f32 * 6.6) as i32;
        let scroll = cam * (1.2 + 0.25 * r);
        let first = (scroll / spacing).floor() as i32 - 1;
        for col in first..first + (SCREEN_W as f32 / spacing) as i32 + 3 {
            let x = (col as f32 * spacing - scroll) as i32 + 8;
            if col.rem_euclid(13) == 0 {
                // Aisle stairs.
                fb.rect(x - 3, y - 4, 7, 10, NAVY[1]);
                for k in 0..3 {
                    fb.rect(x - 3, y - 2 + k * 3, 7, 1, STEEL[1]);
                }
                continue;
            }
            let h = hash(col as u32, row as u32 + 17);
            let cheer = (h >> 4) % 6 == 0 && ((ticks / 14 + (h as u64 & 7)) & 1) == 1;
            let sp = spectator(h, scale, dim, cheer);
            fb.sprite(&sp, x, y + 4);
        }
    }
    // Camera flashes popping in the stands.
    for i in 0..4u64 {
        let t = ticks / 5 + i * 7;
        let h = hash(t as u32, 91 + i as u32);
        if h % 9 < 2 {
            let (fx, fy) = ((h >> 4) as i32 % SCREEN_W as i32, 4 + (h >> 12) as i32 % 28);
            fb.rect(fx - 1, fy, 3, 1, WHITE);
            fb.rect(fx, fy - 1, 1, 3, WHITE);
            fb.px(fx, fy, YELLOW[3]);
        }
    }
    // Barrier wall with courtside advertising boards (these scroll with the floor).
    fb.rect(0, 35, SCREEN_W as i32, 1, NAVY[0]);
    fb.rect(0, 36, SCREEN_W as i32, 12, RED[1]);
    fb.rect(0, 36, SCREEN_W as i32, 1, RED[2]);
    fb.rect(0, 47, SCREEN_W as i32, 1, RED[0]);
    let ad_off = (cam * 4.0) as i32;
    let (ad_base, ad_x) = (ad_off.div_euclid(52), ad_off.rem_euclid(52));
    const ADS: [&str; 4] = ["NTBA", "HOOPS", "SPORTS", "OPEN"];
    for k in 0..=(SCREEN_W as i32 / 52 + 1) {
        let idx = ad_base + k;
        let x = k * 52 - ad_x;
        let white = idx.rem_euclid(2) == 0;
        fb.rect(x + 2, 38, 46, 8, if white { WHITE } else { BLUE[1] });
        fb.bevel(
            x + 2,
            38,
            46,
            8,
            if white { WHITE } else { BLUE[3] },
            if white { STEEL[2] } else { BLUE[0] },
        );
        let label = ADS[idx.rem_euclid(4) as usize];
        fb.text(
            x + 25 - text_width(label, 1) / 2,
            40,
            label,
            if white { BLUE[0] } else { WHITE },
            1,
        );
    }
}

/// Skin, hair and jersey number of each player on a team.
const ROSTERS: [[(usize, usize, u8); 5]; 2] = [
    [(1, 0, 4), (0, 2, 12), (2, 0, 23), (1, 1, 32), (2, 0, 45)],
    [(0, 1, 3), (2, 0, 8), (1, 2, 10), (2, 1, 21), (0, 0, 33)],
];

/// Kit colours of a team: (top, trim, shorts). Home is gold and purple; away is green, or blue in
/// colour-blind mode, because gold and green are nearly the same hue to red-green colour-blind eyes
/// while gold and blue differ in both hue and lightness.
fn kit(team: usize, colourblind: bool) -> (Ramp, Ramp, Ramp) {
    match (team, colourblind) {
        (0, _) => (GOLD, PURPLE, PURPLE),
        (_, false) => (GREEN, CLOTH, GREEN),
        (_, true) => (BLUE, CLOTH, NAVY),
    }
}

fn team_look(team: usize, i: usize, colourblind: bool) -> Look {
    let (skin, hair, number) = ROSTERS[team][i];
    let (top, trim, shorts) = kit(team, colourblind);
    Look {
        skin,
        hair,
        top,
        trim,
        shorts,
        number,
        stripes: false,
        suit: false,
    }
}

/// Light tone of each team's colour, for the controlled-player marker and the scoreboard.
fn team_accent(team: usize, colourblind: bool) -> u8 {
    kit(team, colourblind).0[3]
}
const REF_LOOK: Look = Look {
    skin: 1,
    hair: 0,
    top: CLOTH,
    trim: SUIT,
    shorts: SUIT,
    number: 0,
    stripes: true,
    suit: true,
};
const COACH_LOOK: Look = Look {
    skin: 0,
    hair: 1,
    top: SUIT,
    trim: SUIT,
    shorts: SUIT,
    number: 0,
    stripes: false,
    suit: true,
};

const BENCH_FEET: i32 = 68;

fn draw_bench_area(fb: &mut Framebuffer, cam: f32) {
    fb.rect(0, 48, SCREEN_W as i32, 22, APRON[1]);
    for y in 48..70 {
        for x in 0..SCREEN_W as i32 {
            if (x + y) & 3 == 0 {
                fb.px(x, y, APRON[2]);
            }
        }
    }
    // Home bench in gold warm-ups, visitors in their top colour, coaches in suits.
    for (k, x) in [8.0_f32, 11.5, 15.0, 18.5, 22.0].iter().enumerate() {
        let (sx, _) = project(cam, *x, -1.5, 0.0);
        let l = team_look(0, k, fb.colourblind);
        fb.sprite(&seated_person(l.top, l.skin, l.hair, false), sx, BENCH_FEET);
    }
    let away_top = kit(1, fb.colourblind).0;
    for (k, x) in [66.0_f32, 69.5, 73.0, 76.5, 80.0].iter().enumerate() {
        let (sx, _) = project(cam, *x, -1.5, 0.0);
        fb.sprite(
            &seated_person(away_top, [0, 2, 1, 2, 0][k], k % 3, false),
            sx,
            BENCH_FEET,
        );
    }
    for x in [25.5_f32, 83.5, 87.0] {
        let (sx, _) = project(cam, x, -1.5, 0.0);
        fb.sprite(&seated_person(SUIT, 0, 1, true), sx, BENCH_FEET);
    }
    // Scorer's table with officials behind it.
    let (x0, _) = project(cam, 38.0, -1.5, 0.0);
    let (x1, _) = project(cam, 56.0, -1.5, 0.0);
    for (k, x) in [40.0_f32, 44.0, 48.0, 52.0].iter().enumerate() {
        let (sx, _) = project(cam, *x, -1.5, 0.0);
        let jacket = if k == 1 { CLOTH } else { SUIT };
        fb.sprite(
            &seated_person(jacket, [0, 1, 2, 0][k], k % 3, k != 1),
            sx,
            62,
        );
    }
    fb.rect(x0, 55, x1 - x0, 2, WHITE);
    fb.rect(x0, 57, x1 - x0, 11, PAINT[1]);
    fb.rect(x0, 57, x1 - x0, 1, PAINT[2]);
    let mut x = x0;
    while x < x1 {
        fb.rect(x, 58, 1, 10, PAINT[0]);
        x += 5;
    }
    let mut x = x0;
    while x < x1 {
        fb.rect(x, 68, 3, 1, PAINT[1]);
        x += 6;
    }
    for mx in [x0 + 8, x0 + 26, x0 + 46] {
        fb.rect(mx, 50, 8, 6, STEEL[0]);
        fb.rect(mx + 1, 51, 6, 4, BLUE[1]);
        fb.px(mx + 2, 52, BLUE[3]);
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

/// Arena lighting at a screen pixel (0..1): bright under the lights, falling off at the edges.
fn lighting(sx: i32, sy: i32, mid_row: f32) -> f32 {
    let (dx, dy) = ((sx as f32 - 160.0) / 200.0, (sy as f32 - mid_row) / 110.0);
    (1.0 - 0.55 * dx * dx - 0.3 * dy * dy).clamp(0.0, 1.0)
}

/// Palette index of the floor at a court position (feet), markings included. `kx`/`ky` are the
/// local pixels-per-foot along world x/y so line widths stay about one pixel; (`sx`, `sy`) is the
/// screen pixel, used for dithering and lighting.
fn floor_colour(wx: f32, wy: f32, kx: f32, ky: f32, sx: i32, sy: i32, mid_row: f32) -> u8 {
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
    let light = lighting(sx, sy, mid_row);
    let dither = bayer(sx, sy);
    if in_lane || centre_d < 6.0 {
        let v = 1.15 + (light - 0.6) * 1.3 + dither * 0.6;
        return PAINT[v.floor().clamp(0.0, 2.0) as usize];
    }
    // Hardwood: horizontal planks of random length, each its own tone, with grain and a joint.
    let row = wy as i32;
    let h = hash(row as u32, 7);
    let len = 6.0 + (h % 6) as f32;
    let off = ((h >> 8) % 97) as f32;
    let t = (wx + off) / len;
    let plank = t.floor() as i32;
    if t - t.floor() < 0.25 / len {
        return WOOD[0];
    }
    let tone = [1.6, 2.1, 2.1, 2.6, 3.1, 1.2, 2.1, 3.4]
        [(hash(row as u32, plank as u32 ^ 0xABCD) % 8) as usize];
    let grain = (hash((wy * 3.0) as u32, (wx * 1.7) as u32) % 4) as f32 * 0.13;
    let v = 0.2 + tone * 0.8 + (light - 0.7) * 3.0 + grain + dither;
    WOOD[v.floor().clamp(0.0, 5.0) as usize]
}

fn draw_court(fb: &mut Framebuffer, cam: f32) {
    let mid = COURT_TOP as f32 + COURT_PX_H as f32 * 0.5;
    for sy in COURT_TOP..COURT_TOP + COURT_PX_H {
        let wy = (sy - COURT_TOP) as f32 / PX_PER_FT_Y;
        let s = x_scale(wy);
        for sx in 0..SCREEN_W as i32 {
            let wx = cam + (sx as f32 - 160.0) / s;
            if (0.0..=COURT_L).contains(&wx) {
                fb.px(sx, sy, floor_colour(wx, wy, s, PX_PER_FT_Y, sx, sy, mid));
            }
        }
    }
}

fn draw_court_end(fb: &mut Framebuffer) {
    let mid = (END_BASE_Y + BOTTOM_STRIP as f32) * 0.5;
    for sy in END_BASE_Y as i32..BOTTOM_STRIP {
        let d = (sy as f32 - END_BASE_Y) / END_KY;
        let wx = COURT_L - d;
        let w = end_w(d);
        for sx in 0..SCREEN_W as i32 {
            let wy = COURT_W / 2.0 - (sx as f32 - 160.0) / w;
            if (0.0..=COURT_W).contains(&wy) && wx >= 0.0 {
                fb.px(sx, sy, floor_colour(wx, wy, END_KY, w, sx, sy, mid));
            }
        }
    }
}

// ---------------------------------------------------------------- hoop (side view)

fn hoop_pt(cam: f32, mirror: bool, x: f32, y: f32, z: f32) -> (i32, i32) {
    project(cam, if mirror { COURT_L - x } else { x }, y, z)
}

/// Stanchion, support arm, backboard (seen at an angle) and shot clock.
fn draw_hoop_back(fb: &mut Framebuffer, cam: f32, mirror: bool, shot_clock: f32) {
    let p = |x: f32, y: f32, z: f32| hoop_pt(cam, mirror, x, y, z);
    // Padded base and pole behind the baseline.
    let (bx, by) = p(-3.5, HOOP_Y, 0.0);
    fb.rect(bx - 5, by - 16, 10, 16, PAINT[1]);
    fb.rect(bx - 5, by - 16, 3, 16, PAINT[2]);
    fb.rect(bx + 3, by - 16, 2, 16, PAINT[0]);
    fb.bevel(bx - 5, by - 16, 10, 16, PAINT[2], NAVY[0]);
    let (px, py) = p(-3.5, HOOP_Y, 9.5);
    fb.rect(px - 1, py, 1, by - 16 - py, STEEL[3]);
    fb.rect(px, py, 2, by - 16 - py, STEEL[2]);
    // Arm to the backboard.
    let (ax, ay) = p(1.5, HOOP_Y, 9.5);
    fb.line(px, py, ax, ay, STEEL[3]);
    fb.line(px, py + 1, ax, ay + 1, STEEL[1]);
    // Backboard: a vertical pane from y=22..28 ft, z=9..13 ft.
    let steps = 18;
    for i in 0..=steps {
        let y = 22.0 + 6.0 * i as f32 / steps as f32;
        let (sx, top) = p(1.5, y, 13.0);
        let (_, bot) = p(1.5, y, 9.0);
        let edge = i == 0 || i == steps;
        fb.line(
            sx,
            top,
            sx,
            bot,
            if edge {
                STEEL[2]
            } else if i % 6 == 3 {
                CLOTH[2]
            } else {
                WHITE
            },
        );
    }
    let (l, lt) = p(1.5, 22.0, 13.0);
    let (r, rt) = p(1.5, 28.0, 13.0);
    let (_, lb) = p(1.5, 22.0, 9.0);
    let (_, rb) = p(1.5, 28.0, 9.0);
    fb.line(l, lt, r, rt, STEEL[3]);
    fb.line(l, lb, r, rb, STEEL[1]);
    // Target square.
    let (t0, t0y) = p(1.5, 24.2, 11.6);
    let (t1, t1y) = p(1.5, 25.8, 11.6);
    let (_, b0y) = p(1.5, 24.2, 10.0);
    let (_, b1y) = p(1.5, 25.8, 10.0);
    for (a, b, c, d) in [
        (t0, t0y, t1, t1y),
        (t0, b0y, t1, b1y),
        (t0, t0y, t0, b0y),
        (t1, t1y, t1, b1y),
    ] {
        fb.line(a, b, c, d, RED[2]);
    }
    // Shot clock above the backboard.
    let (cx, cy) = p(1.5, HOOP_Y, 14.0);
    fb.line(cx, cy, cx, cy - 3, STEEL[2]);
    fb.rect(cx - 9, cy - 13, 18, 10, STEEL[2]);
    fb.bevel(cx - 9, cy - 13, 18, 10, STEEL[3], STEEL[0]);
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
    for i in 0..N {
        let (x0, y0) = ring(0.75, RIM_Z, i);
        let (x1, y1) = ring(0.38, RIM_Z - 2.4, i);
        fb.line(x0, y0, x1, y1, if i % 2 == 0 { WHITE } else { STEEL[3] });
    }
    for i in 0..N {
        let (x0, y0) = ring(0.56, RIM_Z - 1.2, i);
        let (x1, y1) = ring(0.56, RIM_Z - 1.2, (i + 1) % N);
        fb.line(x0, y0, x1, y1, STEEL[2]);
    }
    for i in 0..N {
        let (x0, y0) = ring(0.75, RIM_Z, i);
        let (x1, y1) = ring(0.75, RIM_Z, (i + 1) % N);
        fb.line(x0, y0, x1, y1, if i < N / 2 { BALL[2] } else { BALL[1] });
    }
}

// ---------------------------------------------------------------- hoop (end-on view)

/// Coaches, photographers and the referee behind the baseline.
fn draw_end_staff(fb: &mut Framebuffer) {
    for (k, y) in [3.0_f32, 9.0, 41.0, 47.0].iter().enumerate() {
        let (sx, sy) = project_end(97.0, *y, 0.0);
        let mut l = COACH_LOOK;
        l.skin = [0, 2, 1, 0][k];
        fb.sprite(&end_person(&l, false, &STILL, false), sx, sy);
    }
    let (sx, sy) = project_end(95.0, 53.0, 0.0);
    fb.sprite(&end_person(&REF_LOOK, false, &STILL, false), sx, sy);
}

/// Stanchion, backboard (seen face-on) and the shot clock above it. The board is placed
/// relative to the rim, not by true depth, so it stays on screen like in the original.
fn draw_end_hoop_back(fb: &mut Framebuffer, shot_clock: f32) {
    let (rx, ry) = project_end(HOOP_X, HOOP_Y, RIM_Z);
    let (bx, by) = project_end(COURT_L + 3.0, HOOP_Y, 0.0);
    fb.rect(bx - 8, by - 9, 16, 9, PAINT[1]);
    fb.rect(bx - 8, by - 9, 4, 9, PAINT[2]);
    fb.bevel(bx - 8, by - 9, 16, 9, PAINT[2], NAVY[0]);
    fb.rect(rx - 1, ry - 4, 3, by - 9 - (ry - 4), STEEL[2]);
    fb.rect(rx - 14, ry - 19, 29, 16, STEEL[2]);
    fb.rect(rx - 13, ry - 18, 27, 14, WHITE);
    fb.bevel(rx - 14, ry - 19, 29, 16, STEEL[3], STEEL[0]);
    fb.rect(rx - 5, ry - 13, 11, 8, RED[2]);
    fb.rect(rx - 4, ry - 12, 9, 6, WHITE);
    fb.rect(rx - 11, ry - 31, 23, 11, STEEL[2]);
    fb.bevel(rx - 11, ry - 31, 23, 11, STEEL[3], STEEL[0]);
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
        fb.line(x0, y0, x1, y1, if i % 2 == 0 { WHITE } else { STEEL[3] });
    }
    for i in 0..N {
        let (x0, y0) = ring(0.56, RIM_Z - 1.2, i);
        let (x1, y1) = ring(0.56, RIM_Z - 1.2, (i + 1) % N);
        fb.line(x0, y0, x1, y1, STEEL[2]);
    }
    for i in 0..N {
        let (x0, y0) = ring(0.75, RIM_Z, i);
        let (x1, y1) = ring(0.75, RIM_Z, (i + 1) % N);
        fb.line(x0, y0, x1, y1, if i < N / 2 { BALL[2] } else { BALL[1] });
    }
}

// ---------------------------------------------------------------- people and ball

const COACH_X: f32 = 28.0;
const COACH_Y: f32 = -0.6;

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
    // Painter's order: far to near. Side view: small y is far. End view: nearer the basket is far.
    let key = |x: f32, y: f32| match cam.view {
        View::Side => y,
        View::End(Basket::Right) => -x,
        View::End(Basket::Left) => x,
    };
    let mut order: Vec<(f32, Drawable)> = Vec::with_capacity(PLAYERS + 3);
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
        fb.shadow(sx, sy, 7, 2);
    }
    let (cxs, cys) = cam.project(COACH_X, COACH_Y, 0.0);
    let (rx, ry) = cam.project(ref_x, ref_y, 0.0);
    if side {
        fb.shadow(cxs, cys, 7, 2);
        fb.shadow(rx, ry, 7, 2);
    }
    let (bx, by) = cam.project(g.ball.x, g.ball.y, 0.0);
    fb.shadow(bx, by, 3, 1);

    for (_, d) in order {
        match d {
            Drawable::Player(i) => draw_player(g, fb, cam, i),
            Drawable::Referee => fb.sprite(&side_person(&REF_LOOK, 1, &STILL, false), rx, ry),
            Drawable::Coach => fb.sprite(&side_person(&COACH_LOOK, 1, &STILL, false), cxs, cys),
            Drawable::Ball => draw_ball(g, fb, cam),
        }
    }
}

/// Screen position of the ball sprite's centre.
fn ball_screen(g: &Game, cam: Cam) -> (i32, i32) {
    match (cam.view, g.ball.state) {
        // Seen from behind, a dribbled ball would hide behind the body: show it at the hand.
        (View::End(b), BallState::Held(h)) => {
            let p = &g.players[h];
            let (hx, hy) = cam.project(p.x, p.y, 0.0);
            let side = if faces_basket(b, p.facing) { 7 } else { -7 };
            (hx + side, hy - (g.ball.z * END_ZPX) as i32 - 2)
        }
        _ => {
            let (sx, sy) = cam.project(g.ball.x, g.ball.y, g.ball.z);
            (sx, sy - 2)
        }
    }
}

/// True when a player with this facing has their back to the end-on camera.
fn faces_basket(b: Basket, facing: i8) -> bool {
    (facing >= 0) == (b == Basket::Right)
}

fn draw_ball(g: &Game, fb: &mut Framebuffer, cam: Cam) {
    let (sx, sy) = ball_screen(g, cam);
    let mut s = Sprite::new(12, 12, 6, 6);
    s.ellipse(0.0, 0.0, 3.1, 3.1, 0.0, &BALL);
    s.rect(-3, 0, 6, 1, BALL[0]);
    s.rect(0, -3, 1, 6, BALL[0]);
    s.put(-1, -2, BALL[3]);
    s.outline();
    fb.sprite(&s, sx, sy);
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
    let sy = sy - jump_h;
    let handling = matches!(g.ball.state, BallState::Held(h) if h == i);
    let ball_hand = if handling {
        let (bx, by) = ball_screen(g, cam);
        Some(((bx - sx) as f32, (by - sy) as f32))
    } else {
        None
    };
    let pose = Pose {
        phase: p.anim * 0.55,
        moving: (p.vx * p.vx + p.vy * p.vy) > 4.0,
        jump: p.jump > 0.0,
        ball_hand,
    };
    let team = team_of(i);
    let look = &team_look(team, i % TEAM_SIZE, fb.colourblind);
    let controlled = i == g.controlled[team] && g.phase == Phase::Playing;
    match cam.view {
        View::Side => {
            fb.sprite(
                &side_person(look, p.facing as i32, &pose, controlled),
                sx,
                sy,
            );
        }
        View::End(b) => {
            // Moving toward the basket shows the player's back.
            fb.sprite(
                &end_person(look, faces_basket(b, p.facing), &pose, controlled),
                sx,
                sy,
            );
        }
    }
    if controlled && (g.ticks / 20) % 2 == 0 {
        let top = sy - cam.body_h();
        for (k, w) in [7, 5, 3, 1].iter().enumerate() {
            fb.rect(
                sx - w / 2,
                top + k as i32,
                *w,
                1,
                team_accent(team, fb.colourblind),
            );
        }
        fb.rect(sx - 4, top - 1, 9, 1, BLACK);
    }
}

// ---------------------------------------------------------------- HUD

/// Edge arrow in the shirt colour pointing at each off-screen controlled player.
fn draw_offscreen_markers(g: &Game, fb: &mut Framebuffer, cam: Cam) {
    if g.phase != Phase::Playing {
        return;
    }
    const MARGIN: f32 = 7.0;
    for (team, &i) in g.controlled.iter().enumerate() {
        let p = &g.players[i];
        let (sx, sy) = cam.project(p.x, p.y, 0.0);
        let top = sy - cam.body_h();
        if (0..SCREEN_W as i32).contains(&sx) && sy >= 0 && top < BOTTOM_STRIP {
            continue;
        }
        let (bx, by) = (sx as f32, (sy + top) as f32 * 0.5);
        let cx = bx.clamp(MARGIN, SCREEN_W as f32 - MARGIN);
        let cy = by.clamp(MARGIN, BOTTOM_STRIP as f32 - MARGIN);
        let (dx, dy) = (bx - cx, by - cy);
        let len = (dx * dx + dy * dy).sqrt().max(0.001);
        let (ux, uy) = (dx / len, dy / len);
        let tip = (cx + ux * 5.0, cy + uy * 5.0);
        let (mx, my) = (cx - ux * 4.0, cy - uy * 4.0);
        let corners = [
            tip,
            (mx - uy * 5.0, my + ux * 5.0),
            (mx + uy * 5.0, my - ux * 5.0),
        ];
        let shirt = kit(team, fb.colourblind).0;
        fb.triangle(corners, shirt[2]);
        for k in 0..3 {
            let (a, b) = (corners[k], corners[(k + 1) % 3]);
            fb.line(
                a.0.round() as i32,
                a.1.round() as i32,
                b.0.round() as i32,
                b.1.round() as i32,
                shirt[0],
            );
        }
    }
}

fn draw_bottom_strip(fb: &mut Framebuffer) {
    fb.rect(0, BOTTOM_STRIP, SCREEN_W as i32, 16, APRON[0]);
    fb.rect(0, BOTTOM_STRIP, SCREEN_W as i32, 1, APRON[2]);
    fb.text_centered(
        160,
        190,
        "P1 WASD  Z PASS X SHOOT C SWITCH    P2 ARROWS  J K L",
        STEEL[2],
        1,
    );
}

fn draw_scoreboard(g: &Game, fb: &mut Framebuffer) {
    // Compact framed panel in the corner, like the later EA basketball games.
    fb.rect(2, 2, 100, 54, STEEL[2]);
    fb.bevel(2, 2, 100, 54, WHITE, STEEL[0]);
    fb.bevel(3, 3, 98, 52, STEEL[1], STEEL[3]);
    fb.rect(4, 4, 96, 50, BLACK);

    fb.text(10, 6, "TIME", WHITE, 1);
    fb.text(70, 6, "SHOT", WHITE, 1);
    let secs = g.clock.ceil() as u32;
    fb.dot_text(10, 13, &format!("{}:{:02}", secs / 60, secs % 60));
    fb.dot_text(70, 13, &format!("{:02}", g.shot_clock.ceil() as u32));

    fb.text(10, 32, "HOME", team_accent(0, fb.colourblind), 1);
    fb.text(58, 32, "AWAY", team_accent(1, fb.colourblind), 1);
    fb.dot_text(10, 39, &format!("{:03}", g.score[0]));
    fb.dot_text(58, 39, &format!("{:03}", g.score[1]));
}

fn draw_overlays(g: &Game, fb: &mut Framebuffer) {
    if g.phase == Phase::Final {
        fb.rect(70, 104, 180, 44, BLACK);
        fb.bevel(70, 104, 180, 44, STEEL[2], STEEL[0]);
        let result = match g.score[0].cmp(&g.score[1]) {
            core::cmp::Ordering::Greater => "HOME WINS",
            core::cmp::Ordering::Less => "AWAY WINS",
            core::cmp::Ordering::Equal => "TIE GAME",
        };
        fb.text_centered(160, 110, "FINAL", LED, 4);
        fb.text_centered(160, 133, result, WHITE, 1);
        fb.text_centered(160, 140, "PRESS ENTER", STEEL[2], 1);
        return;
    }
    if let Some((msg, _)) = g.message {
        let w = text_width(msg, 2) + 8;
        fb.rect(214 - w / 2, 8, w, 14, BLACK);
        fb.bevel(214 - w / 2, 8, w, 14, STEEL[2], STEEL[0]);
        fb.text_centered(214, 11, msg, WHITE, 2);
    }
}

const DEV_LABEL: &str = "DEVELOPMENT VERSION";

/// Where the "Development Version" plate sits: flush with the top-right corner.
fn dev_badge_rect() -> (i32, i32, i32, i32) {
    let w = text_width(DEV_LABEL, 1) + 6;
    (SCREEN_W as i32 - w, 0, w, 9)
}

/// The plate itself, in the same amber lamps and bevelled steel as the other overlay plates.
/// Drawn last so nothing covers it.
fn draw_dev_badge(fb: &mut Framebuffer) {
    let (x, y, w, h) = dev_badge_rect();
    fb.rect(x, y, w, h, BLACK);
    fb.bevel(x, y, w, h, STEEL[2], STEEL[0]);
    fb.text(x + 3, y + 2, DEV_LABEL, LED, 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{DOWN, LEFT, PASS, RIGHT, SHOOT, UP};

    #[test]
    fn dev_badge_is_drawn_in_the_top_right_corner() {
        let g = Game::new(3);
        let mut fb = Framebuffer::new();
        draw(&g, &mut fb);
        let (x0, y0, w, h) = dev_badge_rect();
        let sw = SCREEN_W as i32;
        assert_eq!(x0 + w, sw, "badge must be flush with the right edge");
        assert_eq!(y0, 0, "badge must be flush with the top edge");
        assert!(w <= 100 && h <= 12, "badge must stay a small corner plate");
        let lit = (y0..y0 + h)
            .flat_map(|y| (x0..x0 + w).map(move |x| (x, y)))
            .filter(|&(x, y)| fb.idx[(y * sw + x) as usize] == LED)
            .count();
        assert!(lit >= 60, "badge text should light many pixels, got {lit}");
    }

    #[test]
    fn dev_badge_is_still_drawn_on_the_final_screen() {
        let mut g = Game::new(3);
        g.phase = Phase::Final; // the overlay returns early; the badge must not
        let mut fb = Framebuffer::new();
        draw(&g, &mut fb);
        let (x0, y0, w, h) = dev_badge_rect();
        let sw = SCREEN_W as i32;
        let lit = (y0..y0 + h)
            .flat_map(|y| (x0..x0 + w).map(move |x| (x, y)))
            .filter(|&(x, y)| fb.idx[(y * sw + x) as usize] == LED)
            .count();
        assert!(lit >= 60, "got {lit}");
    }

    #[test]
    fn renders_every_state_inside_the_palette() {
        let mut g = Game::new(3);
        let mut fb = Framebuffer::new();
        let script = [RIGHT, RIGHT, PASS, 0, 0, SHOOT, 0, 0];
        let away = [LEFT, SHOOT, 0, DOWN, SHOOT, LEFT];
        for t in 0..1500usize {
            g.step([script[(t / 20) % script.len()], away[(t / 17) % away.len()]]);
            if t % 31 == 0 {
                draw(&g, &mut fb);
                assert!(fb.idx.iter().all(|&c| (c as usize) < PALETTE.len()));
            }
        }
        // Not blank, and the alpha channel is opaque.
        assert!(fb.idx.iter().any(|&c| c != APRON[1]));
        assert!(fb.rgba.chunks(4).all(|p| p[3] == 255));
    }

    #[test]
    fn a_frame_uses_a_plausible_number_of_colours() {
        let g = Game::new(3);
        let mut fb = Framebuffer::new();
        draw(&g, &mut fb);
        let mut seen = [false; 256];
        for &c in &fb.idx {
            seen[c as usize] = true;
        }
        let n = seen.iter().filter(|&&s| s).count();
        assert!(n <= MAX_COLOURS);
        assert!(
            n >= 30,
            "the scene should be richly shaded, got {n} colours"
        );
    }

    #[test]
    fn camera_stays_within_its_limits() {
        let mut g = Game::new(1);
        let mut fb = Framebuffer::new();
        for _ in 0..600 {
            g.step([RIGHT, 0]);
            draw(&g, &mut fb);
            assert!((CAM_MIN..=CAM_MAX).contains(&fb.cam_x));
        }
    }

    #[test]
    fn end_on_view_renders_and_cuts_back() {
        let mut g = Game::new(5);
        let mut fb = Framebuffer::new();
        draw(&g, &mut fb);
        let side_frame = fb.idx.clone();
        g.players[0].x = crate::game::END_VIEW_ENTER_X + 4.0;
        g.step([0, 0]);
        assert_eq!(g.view, View::End(Basket::Right));
        for t in 0..120 {
            g.step([if t < 60 { RIGHT } else { SHOOT }, 0]);
            draw(&g, &mut fb);
            assert!(fb.idx.iter().all(|&c| (c as usize) < PALETTE.len()));
        }
        assert_ne!(
            fb.idx, side_frame,
            "the end-on view looks different from the side view"
        );
        g.players[0].x = 40.0;
        g.ball.state = BallState::Held(0);
        g.step([0, 0]);
        assert_eq!(g.view, View::Side);
        draw(&g, &mut fb);
    }

    #[test]
    fn off_screen_controlled_player_gets_an_edge_arrow() {
        let mut g = Game::new(2);
        let mut fb = Framebuffer::new();
        draw(&g, &mut fb);
        let on_screen = fb.idx.clone();
        let away = g.controlled[1];
        g.players[away].x = 90.0;
        g.players[away].y = 25.0;
        draw(&g, &mut fb);
        let edge_changed = (60..BOTTOM_STRIP).any(|y| {
            (SCREEN_W - 12..SCREEN_W).any(|x| {
                let i = y as usize * SCREEN_W + x;
                fb.idx[i] == GREEN[2] && on_screen[i] != GREEN[2]
            })
        });
        assert!(edge_changed, "a green arrow on the right edge");
    }

    #[test]
    fn colourblind_mode_swaps_green_for_blue_on_the_away_team() {
        assert_eq!(team_look(1, 0, false).top, GREEN);
        assert_eq!(team_look(1, 0, true).top, BLUE);
        assert_eq!(team_look(0, 0, true).top, GOLD);
        // Home and away tops must differ clearly in lightness, not just hue.
        let lum = |c: u8| {
            let [r, g, b] = PALETTE[c as usize];
            0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
        };
        assert!((lum(GOLD[2]) - lum(BLUE[2])).abs() > 60.0);
        assert!(team_look(1, 0, true).shorts != PURPLE);
    }

    #[test]
    fn colourblind_arrow_is_blue_not_green() {
        let mut g = Game::new(2);
        let mut fb = Framebuffer::new();
        fb.colourblind = true;
        let away = g.controlled[1];
        g.players[away].x = 90.0;
        g.players[away].y = 25.0;
        draw(&g, &mut fb);
        let edge = |c: u8| {
            (60..BOTTOM_STRIP)
                .any(|y| (SCREEN_W - 12..SCREEN_W).any(|x| fb.idx[y as usize * SCREEN_W + x] == c))
        };
        assert!(edge(BLUE[2]), "a blue arrow on the right edge");
        assert!(!edge(GREEN[2]));
    }

    #[test]
    fn left_end_view_renders_inside_the_palette() {
        let mut g = Game::new(5);
        let mut fb = Framebuffer::new();
        g.reset_possession(1);
        g.players[g.controlled[1]].x = COURT_L - crate::game::END_VIEW_ENTER_X - 4.0;
        g.step([0, 0]);
        assert_eq!(g.view, View::End(Basket::Left));
        for t in 0..120 {
            g.step([0, if t < 60 { UP } else { SHOOT }]);
            draw(&g, &mut fb);
            assert!(fb.idx.iter().all(|&c| (c as usize) < PALETTE.len()));
        }
    }
}
