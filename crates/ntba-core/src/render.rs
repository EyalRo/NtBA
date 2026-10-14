//! Software renderer: draws one frame of the game into a 320x200 paletted
//! framebuffer, then expands it to RGBA for the host to blit.

use crate::font::{glyph, text_width, GLYPH_W};
use crate::game::{BallState, Game, Phase, HOOP_Y, RIM_Z};
use crate::{COURT_L, COURT_W, SCREEN_H, SCREEN_W};

// Palette indices. The whole game fits in 16 colours.
pub const BLACK: u8 = 0;
pub const WHITE: u8 = 1;
pub const WOOD_A: u8 = 2;
pub const WOOD_B: u8 = 3;
pub const PAINT: u8 = 4;
pub const GRAY: u8 = 6;
pub const DARK: u8 = 7;
pub const SKIN: u8 = 8;
pub const SHADOW: u8 = 9;
pub const JERSEY: u8 = 10;
pub const SHORTS: u8 = 11;
pub const CROWD_A: u8 = 12;
pub const CROWD_B: u8 = 13;
pub const CROWD_C: u8 = 14;
pub const BALL: u8 = 15;

/// RGB for each palette index.
pub const PALETTE: [[u8; 3]; 16] = [
    [0, 0, 0],
    [252, 252, 252],
    [200, 140, 64],
    [184, 124, 52],
    [160, 44, 44],
    [0, 0, 0],
    [136, 136, 136],
    [52, 52, 64],
    [240, 176, 128],
    [120, 76, 32],
    [244, 200, 32],
    [100, 44, 164],
    [200, 64, 64],
    [64, 96, 200],
    [64, 160, 80],
    [232, 104, 24],
];

// Screen layout.
const COURT_TOP: i32 = 74;
const COURT_PX_H: i32 = 100;
const SCOREBOARD_H: i32 = 30;
const Z_PX: f32 = 3.0; // screen pixels per foot of height

pub struct Framebuffer {
    pub idx: Vec<u8>,
    pub rgba: Vec<u8>,
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

    fn expand(&mut self) {
        for (i, &c) in self.idx.iter().enumerate() {
            let rgb = PALETTE[(c & 15) as usize];
            self.rgba[i * 4] = rgb[0];
            self.rgba[i * 4 + 1] = rgb[1];
            self.rgba[i * 4 + 2] = rgb[2];
            self.rgba[i * 4 + 3] = 255;
        }
    }
}

/// Project a court position (feet) and height into screen pixels.
pub fn project(x: f32, y: f32, z: f32) -> (i32, i32) {
    let s = 3.0 + 0.4 * (y / COURT_W);
    let sx = 160.0 + (x - COURT_L / 2.0) * s;
    let sy = COURT_TOP as f32 + y * (COURT_PX_H as f32 / COURT_W) - z * Z_PX;
    (sx as i32, sy as i32)
}

pub fn draw(g: &Game, fb: &mut Framebuffer) {
    fb.rect(0, 0, SCREEN_W as i32, SCREEN_H as i32, DARK);
    draw_crowd(fb, g.ticks);
    draw_court(fb);
    draw_backboards(fb);
    draw_entities(g, fb);
    draw_rims(fb);
    draw_bench_strip(fb);
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

fn draw_crowd(fb: &mut Framebuffer, ticks: u64) {
    let colors = [CROWD_A, CROWD_B, CROWD_C, JERSEY, WHITE];
    for row in 0..6 {
        let y0 = SCOREBOARD_H + 3 + row * 7;
        for col in 0..(SCREEN_W as i32 / 5) {
            let h = hash(col as u32, row as u32);
            let cheer = if (h >> 3) % 5 == 0 {
                ((ticks / 20 + (h as u64 & 7)) & 1) as i32
            } else {
                0
            };
            let x = col * 5 + 1;
            let body = colors[(h % 5) as usize];
            fb.rect(x, y0 + 3 - cheer, 3, 4, body);
            fb.rect(
                x,
                y0 - cheer,
                3,
                3,
                if h & 0x100 != 0 { SKIN } else { SHADOW },
            );
        }
    }
    fb.rect(0, 70, SCREEN_W as i32, 3, GRAY);
}

fn near(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() < tol
}

/// Colour of the hardwood at a court position, markings included.
fn floor_colour(wx: f32, wy: f32) -> u8 {
    let mx = wx.min(COURT_L - wx); // everything but the centre line is symmetric
    let (dxh, dyh) = (mx - 5.25, wy - HOOP_Y);
    let hoop_d = (dxh * dxh + dyh * dyh).sqrt();
    let (dxf, dyf) = (mx - 19.0, wy - HOOP_Y);
    let ft_d = (dxf * dxf + dyf * dyf).sqrt();
    let (dxc, dyc) = (wx - COURT_L / 2.0, wy - HOOP_Y);
    let centre_d = (dxc * dxc + dyc * dyc).sqrt();

    let line = near(wy, 0.4, 0.5)
        || near(wy, COURT_W - 0.4, 0.5)
        || mx < 0.7
        || near(wx, COURT_L / 2.0, 0.4)
        || near(centre_d, 6.0, 0.5)
        || (mx < 19.0 && (near(wy, 17.0, 0.5) || near(wy, 33.0, 0.5)))
        || (near(mx, 19.0, 0.4) && (wy - HOOP_Y).abs() < 8.0)
        || (mx >= 19.0 && near(ft_d, 6.0, 0.5))
        || (near(hoop_d, 23.75, 0.5) && (wy - HOOP_Y).abs() < 22.0 && mx > 5.25)
        || (mx < 14.0 && (near(wy, 3.0, 0.5) || near(wy, COURT_W - 3.0, 0.5)));
    if line {
        return WHITE;
    }
    if mx < 19.0 && (wy - HOOP_Y).abs() < 8.0 {
        return PAINT;
    }
    if (wy * 1.6) as i32 & 1 == 0 {
        WOOD_A
    } else {
        WOOD_B
    }
}

fn draw_court(fb: &mut Framebuffer) {
    for sy in COURT_TOP..COURT_TOP + COURT_PX_H {
        let wy = (sy - COURT_TOP) as f32 * (COURT_W / COURT_PX_H as f32);
        let s = 3.0 + 0.4 * (wy / COURT_W);
        for sx in 0..SCREEN_W as i32 {
            let wx = COURT_L / 2.0 + (sx as f32 - 160.0) / s;
            if (0.0..=COURT_L).contains(&wx) {
                fb.px(sx, sy, floor_colour(wx, wy));
            } else {
                fb.px(sx, sy, DARK);
            }
        }
    }
}

/// Backboards seen edge-on from the sideline, plus their support poles.
fn draw_backboards(fb: &mut Framebuffer) {
    for &x in &[1.5_f32, COURT_L - 1.5] {
        let (sx, sy_floor) = project(x, HOOP_Y, 0.0);
        let (_, sy_top) = project(x, HOOP_Y, RIM_Z + 3.0);
        let (_, sy_bot) = project(x, HOOP_Y, RIM_Z - 1.0);
        fb.rect(sx, sy_bot, 1, sy_floor - sy_bot, GRAY);
        fb.rect(sx - 1, sy_top, 3, sy_bot - sy_top, WHITE);
        fb.rect(sx - 1, sy_top, 3, 1, GRAY);
    }
}

fn draw_rims(fb: &mut Framebuffer) {
    for &x in &[5.25_f32, COURT_L - 5.25] {
        let (sx, sy) = project(x, HOOP_Y, RIM_Z);
        fb.rect(sx - 3, sy, 6, 1, BALL);
        for k in 1..=4 {
            let w = 6 - k;
            fb.rect(
                sx - w / 2,
                sy + k,
                w.max(2),
                1,
                if k % 2 == 0 { WHITE } else { GRAY },
            );
        }
    }
}

enum Drawable {
    Player(usize),
    Ball,
}

fn draw_entities(g: &Game, fb: &mut Framebuffer) {
    let mut order: Vec<(f32, Drawable)> = Vec::with_capacity(6);
    for (i, p) in g.players.iter().enumerate() {
        order.push((p.y, Drawable::Player(i)));
    }
    let ball_key = match g.ball.state {
        BallState::Held(h) => g.players[h].y + 0.01,
        _ => g.ball.y,
    };
    order.push((ball_key, Drawable::Ball));
    order.sort_by(|a, b| a.0.total_cmp(&b.0));

    // Shadows first so nobody is painted over by someone else's shadow.
    for p in &g.players {
        let (sx, sy) = project(p.x, p.y, 0.0);
        fb.rect(sx - 4, sy - 1, 9, 2, SHADOW);
    }
    let (bx, by) = project(g.ball.x, g.ball.y, 0.0);
    fb.rect(bx - 1, by, 3, 1, SHADOW);

    draw_referee(g, fb);
    for (_, d) in order {
        match d {
            Drawable::Player(i) => draw_player(g, fb, i),
            Drawable::Ball => draw_ball(g, fb),
        }
    }
}

fn draw_ball(g: &Game, fb: &mut Framebuffer) {
    let (sx, sy) = project(g.ball.x, g.ball.y, g.ball.z);
    let (sx, sy) = (sx, sy - 2);
    fb.rect(sx - 1, sy - 2, 2, 1, BALL);
    fb.rect(sx - 2, sy - 1, 4, 2, BALL);
    fb.rect(sx - 1, sy + 1, 2, 1, BALL);
    fb.px(sx - 1, sy - 1, BLACK);
}

fn draw_referee(g: &Game, fb: &mut Framebuffer) {
    let rx = g.ball.x.clamp(8.0, COURT_L - 8.0);
    let (sx, sy) = project(rx, COURT_W + 2.5, 0.0);
    fb.rect(sx - 4, sy - 1, 9, 2, SHADOW);
    for row in 0..9 {
        fb.rect(
            sx - 2,
            sy - 9 + row,
            5,
            1,
            if row % 2 == 0 { BLACK } else { WHITE },
        );
    }
    fb.rect(sx - 1, sy - 13, 3, 4, SKIN);
    fb.rect(sx - 2, sy - 14, 5, 1, BLACK);
    fb.rect(sx - 2, sy, 2, 3, BLACK);
    fb.rect(sx + 1, sy, 2, 3, BLACK);
}

fn draw_player(g: &Game, fb: &mut Framebuffer, i: usize) {
    let p = &g.players[i];
    let jump_h = if p.jump > 0.0 {
        let t = 1.0 - p.jump / 0.5;
        (4.0 * t * (1.0 - t) * 2.4 * Z_PX) as i32
    } else {
        0
    };
    let (sx, sy) = project(p.x, p.y, 0.0);
    let sy = sy - jump_h;
    let f = p.facing as i32;
    // Mirror helper: rectangle spanning dx0..=dx1 pixels in front of the body centre.
    let mut r = |dx0: i32, dx1: i32, top: i32, bot: i32, c: u8| {
        let (a, b) = if f > 0 {
            (sx + dx0, sx + dx1)
        } else {
            (sx - dx1, sx - dx0)
        };
        fb.rect(a, sy + top, b - a + 1, bot - top + 1, c);
    };

    let frame = ((p.anim * 0.55) as i32) & 3;
    let moving = (p.vx * p.vx + p.vy * p.vy) > 4.0;
    let airborne = p.jump > 0.0;

    // Legs (rows -6..-1), shoes on the last row.
    let (back_top, front_top) = if airborne {
        (-5, -5)
    } else if !moving {
        (-6, -6)
    } else {
        match frame {
            1 => (-6, -4),
            3 => (-4, -6),
            _ => (-6, -6),
        }
    };
    r(-2, -1, back_top, -1, SKIN);
    r(0, 1, front_top, -1, SKIN);
    r(-2, -1, -1, -1, WHITE);
    r(0, 1, -1, -1, WHITE);
    // Shorts and jersey.
    r(-3, 2, -9, -6, SHORTS);
    r(-3, 2, -16, -10, JERSEY);
    // Head.
    r(-1, 2, -20, -17, SKIN);
    r(-1, 2, -21, -20, BLACK);
    // Arms.
    if airborne {
        r(1, 3, -21, -14, SKIN);
    } else if matches!(g.ball.state, BallState::Held(h) if h == i) {
        r(2, 4, -14, -13, SKIN);
        r(-4, -4, -15, -12, SKIN);
    } else {
        let swing = if moving && (frame == 1 || frame == 3) {
            1
        } else {
            0
        };
        r(-4 - swing, -4 - swing, -15, -11, SKIN);
        r(3 + swing, 3 + swing, -15, -11, SKIN);
    }

    // Marker over the player the human controls.
    if i == g.controlled && g.phase == Phase::Playing && (g.ticks / 20) % 2 == 0 {
        let top = sy - 28;
        fb.rect(sx - 2, top, 5, 1, WHITE);
        fb.rect(sx - 1, top + 1, 3, 1, WHITE);
        fb.px(sx, top + 2, WHITE);
    }
}

fn draw_bench_strip(fb: &mut Framebuffer) {
    fb.rect(0, 176, SCREEN_W as i32, 2, GRAY);
    // A bench with a few seated players.
    fb.rect(12, 186, 70, 3, SHADOW);
    for k in 0..5 {
        let x = 16 + k * 13;
        fb.rect(x, 178, 5, 3, SKIN);
        fb.rect(x - 1, 181, 7, 5, if k % 2 == 0 { JERSEY } else { SHORTS });
    }
    fb.text(100, 183, "ARROWS MOVE", WHITE, 1);
    fb.text(100, 190, "X SHOOT  Z PASS  C SWITCH", GRAY, 1);
}

fn draw_scoreboard(g: &Game, fb: &mut Framebuffer) {
    let w = SCREEN_W as i32;
    fb.rect(0, 0, w, SCOREBOARD_H, DARK);
    fb.rect(0, SCOREBOARD_H - 1, w, 1, GRAY);
    fb.rect(0, 0, w, 1, GRAY);

    // Home score.
    fb.text(10, 4, "HOME", WHITE, 1);
    fb.rect(8, 11, 50, 16, BLACK);
    fb.text(12, 13, &format!("{:02}", g.score), JERSEY, 3);

    // Game clock.
    let secs = g.clock.ceil() as u32;
    fb.text_centered(160, 3, "NTBA", GRAY, 1);
    fb.rect(122, 10, 76, 17, BLACK);
    fb.text_centered(
        160,
        13,
        &format!("{}:{:02}", secs / 60, secs % 60),
        JERSEY,
        3,
    );

    // Shot clock.
    fb.text(256, 4, "SHOT", WHITE, 1);
    fb.rect(254, 11, 50, 16, BLACK);
    fb.text(
        258,
        13,
        &format!("{:02}", g.shot_clock.ceil() as u32),
        CROWD_A,
        3,
    );
}

fn draw_overlays(g: &Game, fb: &mut Framebuffer) {
    if g.phase == Phase::Final {
        fb.rect(70, 100, 180, 44, BLACK);
        fb.text_centered(160, 106, "FINAL", JERSEY, 4);
        fb.text_centered(160, 130, "PRESS ENTER", WHITE, 1);
        return;
    }
    if let Some((msg, _)) = g.message {
        let w = text_width(msg, 2) + 8;
        fb.rect(160 - w / 2, 34, w, 14, BLACK);
        fb.text_centered(160, 37, msg, WHITE, 2);
    }
}
