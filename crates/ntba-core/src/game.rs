//! Deterministic single-team simulation for the proof of concept.
//!
//! One team of five attacks the right-hand basket with no opposition. The
//! human controls one player at a time; the other four jog to offensive spots.
//! Only `+ - * /` and `sqrt` on `f32` are used so a future lock-step P2P build
//! stays bit-identical across machines.

use crate::input::{self, PASS, SHOOT, START, SWITCH};
use crate::{COURT_L, COURT_W, DT};

pub const HOOP_X: f32 = COURT_L - 5.25;
pub const HOOP_Y: f32 = COURT_W / 2.0;
pub const RIM_Z: f32 = 10.0;
pub const THREE_POINT_DIST: f32 = 23.75;
pub const GRAVITY: f32 = 64.0;
pub const SHOT_CLOCK_SECS: f32 = 24.0;
pub const PERIOD_SECS: f32 = 300.0;
/// The camera cuts to the end-on view once the ball is this close to the attacked basket...
pub const END_VIEW_ENTER_X: f32 = 66.0;
/// ...and back to the side view when it drops below this (hysteresis avoids flicker).
pub const END_VIEW_EXIT_X: f32 = 58.0;
const DIR_MASK: u8 = input::UP | input::DOWN | input::LEFT | input::RIGHT;
const SCORED_PAUSE: f32 = 1.8;
const MESSAGE_SECS: f32 = 1.6;

/// Where the five players line up at the start of a possession, ball with player 0.
const START_SPOTS: [(f32, f32); 5] = [
    (30.0, 25.0),
    (36.0, 10.0),
    (36.0, 40.0),
    (44.0, 18.0),
    (44.0, 32.0),
];
/// Half-court spots the AI teammates run to.
const OFFENSE_SPOTS: [(f32, f32); 5] = [
    (66.0, 25.0),
    (72.0, 8.0),
    (72.0, 42.0),
    (84.0, 13.0),
    (84.0, 37.0),
];

#[derive(Clone, Copy, Debug, Default)]
pub struct Player {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    /// +1 faces right, -1 faces left.
    pub facing: i8,
    /// Accumulated distance run; drives the walk cycle.
    pub anim: f32,
    /// Seconds left in a jump-shot animation (0 when grounded).
    pub jump: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BallState {
    Held(usize),
    Pass { to: usize },
    Shot { t: f32, dur: f32, made: bool },
    Loose,
    Scored { t: f32 },
}

#[derive(Clone, Copy, Debug)]
pub struct Ball {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub state: BallState,
}

/// Which camera is showing the play. The simulation owns this because it changes what the
/// direction keys mean (on the end-on view "up" runs toward the basket).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// Sideline view that scrolls along the court (transition and backcourt play).
    Side,
    /// Looking at the basket from midcourt (half-court offence).
    End,
}

/// Translate screen-relative direction bits into court axes (`UP` = -y, `RIGHT` = +x).
fn map_dir(bits: u8, view: View) -> u8 {
    match view {
        View::Side => bits,
        View::End => {
            let mut out = 0;
            if bits & input::UP != 0 {
                out |= input::RIGHT; // toward the basket
            }
            if bits & input::DOWN != 0 {
                out |= input::LEFT;
            }
            if bits & input::LEFT != 0 {
                out |= input::DOWN; // screen-left is +y when looking down the court
            }
            if bits & input::RIGHT != 0 {
                out |= input::UP;
            }
            out
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Playing,
    Final,
}

pub struct Game {
    pub players: [Player; 5],
    pub ball: Ball,
    pub controlled: usize,
    pub score: u32,
    pub clock: f32,
    pub shot_clock: f32,
    pub phase: Phase,
    pub view: View,
    /// View whose key mapping stays in force for a direction held across a camera cut.
    legacy_view: Option<View>,
    pub message: Option<(&'static str, f32)>,
    pub ticks: u64,
    rng: u32,
    prev_input: u8,
    pending_points: u32,
}

impl Game {
    pub fn new(seed: u32) -> Self {
        let mut g = Game {
            players: [Player::default(); 5],
            ball: Ball {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                vx: 0.0,
                vy: 0.0,
                vz: 0.0,
                state: BallState::Held(0),
            },
            controlled: 0,
            score: 0,
            clock: PERIOD_SECS,
            shot_clock: SHOT_CLOCK_SECS,
            phase: Phase::Playing,
            view: View::Side,
            legacy_view: None,
            message: None,
            ticks: 0,
            rng: seed.wrapping_mul(0x9E37_79B1) | 1,
            prev_input: 0,
            pending_points: 2,
        };
        for _ in 0..8 {
            g.rand();
        }
        g.reset_possession();
        g
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / 16_777_216.0
    }

    pub fn reset_possession(&mut self) {
        for (p, &(x, y)) in self.players.iter_mut().zip(START_SPOTS.iter()) {
            *p = Player {
                x,
                y,
                facing: 1,
                ..Player::default()
            };
        }
        self.controlled = 0;
        self.view = View::Side;
        self.legacy_view = None;
        self.shot_clock = SHOT_CLOCK_SECS;
        self.ball.state = BallState::Held(0);
        self.ball.vx = 0.0;
        self.ball.vy = 0.0;
        self.ball.vz = 0.0;
        self.sync_held_ball();
    }

    fn restart(&mut self) {
        self.score = 0;
        self.clock = PERIOD_SECS;
        self.phase = Phase::Playing;
        self.message = None;
        self.reset_possession();
    }

    fn say(&mut self, text: &'static str) {
        self.message = Some((text, MESSAGE_SECS));
    }

    /// Advance the simulation by one fixed tick.
    pub fn step(&mut self, buttons: u8) {
        let pressed = buttons & !self.prev_input;
        self.prev_input = buttons;
        self.ticks += 1;

        if let Some((text, left)) = self.message {
            self.message = if left > DT {
                Some((text, left - DT))
            } else {
                None
            };
        }

        if self.phase == Phase::Final {
            if pressed & START != 0 {
                self.restart();
            }
            return;
        }

        self.update_clocks();
        let raw = buttons;
        let buttons = self.remap_buttons(raw);
        self.handle_actions(pressed, buttons);
        self.move_players(buttons);
        self.update_ball();
        self.update_view(raw);
    }

    /// Apply the current camera's direction mapping to the raw key bitmask.
    fn remap_buttons(&mut self, raw: u8) -> u8 {
        let dirs = raw & DIR_MASK;
        if dirs == 0 {
            self.legacy_view = None;
        }
        let view = self.legacy_view.unwrap_or(self.view);
        (raw & !DIR_MASK) | map_dir(dirs, view)
    }

    /// Cut between the side and end-on cameras based on where the ball is.
    fn update_view(&mut self, raw: u8) {
        let next = match self.view {
            View::Side if self.ball.x >= END_VIEW_ENTER_X => View::End,
            View::End if self.ball.x <= END_VIEW_EXIT_X => View::Side,
            v => v,
        };
        if next != self.view {
            // Keep a held direction meaning what it meant until the player lets go.
            if raw & DIR_MASK != 0 {
                self.legacy_view = Some(self.view);
            }
            self.view = next;
        }
    }

    fn update_clocks(&mut self) {
        self.clock -= DT;
        if self.clock <= 0.0 {
            self.clock = 0.0;
            self.phase = Phase::Final;
            return;
        }
        if !matches!(self.ball.state, BallState::Scored { .. }) {
            self.shot_clock -= DT;
            if self.shot_clock <= 0.0 {
                self.reset_possession();
                self.say("SHOT CLOCK");
            }
        }
    }

    fn handle_actions(&mut self, pressed: u8, buttons: u8) {
        if pressed & SWITCH != 0
            && !matches!(self.ball.state, BallState::Held(_) | BallState::Pass { .. })
        {
            self.controlled =
                self.nearest_player_to(self.ball.x, self.ball.y, Some(self.controlled));
        }
        if let BallState::Held(h) = self.ball.state {
            if pressed & SHOOT != 0 {
                self.shoot(h);
            } else if pressed & PASS != 0 {
                self.pass(h, buttons);
            }
        }
    }

    fn nearest_player_to(&self, x: f32, y: f32, skip: Option<usize>) -> usize {
        let mut best = 0;
        let mut best_d = f32::MAX;
        for (i, p) in self.players.iter().enumerate() {
            if Some(i) == skip {
                continue;
            }
            let d = dist2(p.x, p.y, x, y);
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        best
    }

    fn shoot(&mut self, h: usize) {
        let p = self.players[h];
        let d = dist(p.x, p.y, HOOP_X, HOOP_Y);
        let chance = (0.95 - 0.022 * d).clamp(0.12, 0.9);
        let made = self.rand() < chance;
        let dur = 0.55 + d * 0.012;
        let (tx, ty) = if made {
            (HOOP_X, HOOP_Y)
        } else {
            let ox = (self.rand() - 0.5) * 3.0;
            let oy = (self.rand() - 0.5) * 3.0;
            (HOOP_X + ox, HOOP_Y + oy)
        };
        let z0 = 7.0;
        self.ball.x = p.x;
        self.ball.y = p.y;
        self.ball.z = z0;
        self.ball.vx = (tx - p.x) / dur;
        self.ball.vy = (ty - p.y) / dur;
        self.ball.vz = (RIM_Z - z0) / dur + 0.5 * GRAVITY * dur;
        self.ball.state = BallState::Shot { t: 0.0, dur, made };
        self.players[h].jump = 0.5;
        self.players[h].facing = 1;
        // Remember the value of the shot for scoring.
        self.pending_points = if d >= THREE_POINT_DIST { 3 } else { 2 };
    }

    fn pass(&mut self, h: usize, buttons: u8) {
        let (dx, dy) = input::dir(buttons);
        let have_dir = dx != 0.0 || dy != 0.0;
        let len = (dx * dx + dy * dy).sqrt();
        let me = self.players[h];
        let mut target = None;
        let mut best = f32::MIN;
        for (i, p) in self.players.iter().enumerate() {
            if i == h {
                continue;
            }
            let d = dist(me.x, me.y, p.x, p.y).max(0.001);
            let score = if have_dir {
                ((p.x - me.x) * dx + (p.y - me.y) * dy) / (d * len) - d * 0.002
            } else {
                -d
            };
            if score > best {
                best = score;
                target = Some(i);
            }
        }
        if let Some(to) = target {
            self.ball.state = BallState::Pass { to };
            self.controlled = to;
        }
    }

    #[allow(clippy::needless_range_loop)] // indexes players and OFFENSE_SPOTS in lockstep
    fn move_players(&mut self, buttons: u8) {
        let (ix, iy) = input::dir(buttons);
        let handler = match self.ball.state {
            BallState::Held(h) => Some(h),
            _ => None,
        };
        let chaser = if self.ball.state == BallState::Loose {
            Some(self.nearest_player_to(self.ball.x, self.ball.y, None))
        } else {
            None
        };

        for i in 0..5 {
            let (mut tx, mut ty);
            if i == self.controlled {
                let len = (ix * ix + iy * iy).sqrt();
                let s = if handler == Some(i) { 17.0 } else { 20.0 };
                if len > 0.0 {
                    tx = ix / len * s;
                    ty = iy / len * s;
                } else {
                    tx = 0.0;
                    ty = 0.0;
                }
            } else {
                let (gx, gy) = if chaser == Some(i) {
                    (self.ball.x, self.ball.y)
                } else {
                    OFFENSE_SPOTS[i]
                };
                let p = &self.players[i];
                let (dx, dy) = (gx - p.x, gy - p.y);
                let d = (dx * dx + dy * dy).sqrt();
                let speed = if chaser == Some(i) { 20.0 } else { 15.0 };
                if d > 1.0 {
                    tx = dx / d * speed;
                    ty = dy / d * speed;
                } else {
                    tx = 0.0;
                    ty = 0.0;
                }
            }
            if self.players[i].jump > 0.0 {
                tx = 0.0;
                ty = 0.0;
            }
            let p = &mut self.players[i];
            let accel = 140.0 * DT;
            p.vx = approach(p.vx, tx, accel);
            p.vy = approach(p.vy, ty, accel);
            p.x = (p.x + p.vx * DT).clamp(1.0, COURT_L - 1.0);
            p.y = (p.y + p.vy * DT).clamp(1.0, COURT_W - 1.0);
            p.anim += (p.vx * p.vx + p.vy * p.vy).sqrt() * DT;
            if p.vx > 1.0 {
                p.facing = 1;
            } else if p.vx < -1.0 {
                p.facing = -1;
            }
            if p.jump > 0.0 {
                p.jump = (p.jump - DT).max(0.0);
            }
        }
        self.separate_players();
    }

    /// Keep teammates from stacking on the same spot.
    fn separate_players(&mut self) {
        for i in 0..5 {
            for j in (i + 1)..5 {
                let (dx, dy) = (
                    self.players[j].x - self.players[i].x,
                    self.players[j].y - self.players[i].y,
                );
                let d2 = dx * dx + dy * dy;
                if d2 < 4.0 && d2 > 0.0001 {
                    let d = d2.sqrt();
                    let push = (2.0 - d) * 0.5;
                    let (nx, ny) = (dx / d * push, dy / d * push);
                    self.players[i].x -= nx;
                    self.players[i].y -= ny;
                    self.players[j].x += nx;
                    self.players[j].y += ny;
                }
            }
        }
    }

    fn sync_held_ball(&mut self) {
        if let BallState::Held(h) = self.ball.state {
            let p = self.players[h];
            // Triangle wave dribble, a little faster when running.
            let speed = (p.vx * p.vx + p.vy * p.vy).sqrt();
            let rate = 2.2 + speed * 0.05;
            let phase = ((self.ticks as f32) * DT * rate).fract();
            let tri = if phase < 0.5 {
                phase * 2.0
            } else {
                2.0 - phase * 2.0
            };
            self.ball.x = p.x + p.facing as f32 * 1.4;
            self.ball.y = p.y + 0.3;
            self.ball.z = 0.6 + 3.0 * tri;
        }
    }

    fn update_ball(&mut self) {
        match self.ball.state {
            BallState::Held(_) => self.sync_held_ball(),
            BallState::Pass { to } => self.update_pass(to),
            BallState::Shot { t, dur, made } => self.update_shot(t, dur, made),
            BallState::Loose => self.update_loose(),
            BallState::Scored { t } => self.update_scored(t),
        }
    }

    fn update_pass(&mut self, to: usize) {
        let r = self.players[to];
        let (tx, ty) = (r.x + r.vx * 0.15, r.y + r.vy * 0.15);
        let (dx, dy) = (tx - self.ball.x, ty - self.ball.y);
        let d = (dx * dx + dy * dy).sqrt();
        let step = 55.0 * DT;
        if d <= step.max(1.8) {
            self.ball.state = BallState::Held(to);
            self.controlled = to;
            self.sync_held_ball();
            return;
        }
        self.ball.x += dx / d * step;
        self.ball.y += dy / d * step;
        self.ball.z = approach(self.ball.z, 4.0, 12.0 * DT);
    }

    fn update_shot(&mut self, t: f32, dur: f32, made: bool) {
        self.integrate_ball();
        let t = t + DT;
        if t < dur {
            self.ball.state = BallState::Shot { t, dur, made };
            return;
        }
        if made {
            self.ball.x = HOOP_X;
            self.ball.y = HOOP_Y;
            self.ball.z = RIM_Z;
            self.ball.vx = 0.0;
            self.ball.vy = 0.0;
            self.ball.vz = -6.0;
            self.score += self.pending_points;
            self.say(if self.pending_points == 3 {
                "THREE POINTS!"
            } else {
                "SCORE!"
            });
            self.ball.state = BallState::Scored { t: 0.0 };
        } else {
            // Rim it out: kick back toward the court with a bit of randomness.
            self.ball.z = RIM_Z;
            self.ball.vx = -(3.0 + self.rand() * 6.0);
            self.ball.vy = (self.rand() - 0.5) * 8.0;
            self.ball.vz = 12.0;
            self.ball.state = BallState::Loose;
            self.shot_clock = SHOT_CLOCK_SECS;
            self.say("MISS");
            self.controlled = self.nearest_player_to(self.ball.x - 10.0, self.ball.y, None);
        }
    }

    fn update_loose(&mut self) {
        self.integrate_ball();
        let grounded = self.ball.z <= 0.0 && self.ball.vz.abs() < 0.1;
        if grounded {
            let f = 1.0 - 1.6 * DT;
            self.ball.vx *= f;
            self.ball.vy *= f;
        }
        if self.ball.z < 7.5 {
            let i = self.nearest_player_to(self.ball.x, self.ball.y, None);
            let p = self.players[i];
            if dist(p.x, p.y, self.ball.x, self.ball.y) < 2.6 {
                self.ball.state = BallState::Held(i);
                self.controlled = i;
                self.sync_held_ball();
            }
        }
    }

    fn update_scored(&mut self, t: f32) {
        self.integrate_ball();
        let t = t + DT;
        if t >= SCORED_PAUSE {
            self.reset_possession();
        } else {
            self.ball.state = BallState::Scored { t };
        }
    }

    /// Ballistic motion with floor bounce and court-edge bounce.
    fn integrate_ball(&mut self) {
        let b = &mut self.ball;
        b.vz -= GRAVITY * DT;
        b.x += b.vx * DT;
        b.y += b.vy * DT;
        b.z += b.vz * DT;
        if b.z <= 0.0 {
            b.z = 0.0;
            b.vz = if b.vz < -2.5 { -b.vz * 0.62 } else { 0.0 };
        }
        if b.x < 0.0 || b.x > COURT_L {
            b.x = b.x.clamp(0.0, COURT_L);
            b.vx = -b.vx * 0.5;
        }
        if b.y < 0.0 || b.y > COURT_W {
            b.y = b.y.clamp(0.0, COURT_W);
            b.vy = -b.vy * 0.5;
        }
    }
}

fn approach(v: f32, target: f32, step: f32) -> f32 {
    if v < target - step {
        v + step
    } else if v > target + step {
        v - step
    } else {
        target
    }
}

fn dist2(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (ax - bx, ay - by);
    dx * dx + dy * dy
}

fn dist(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    dist2(ax, ay, bx, by).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{DOWN, RIGHT};

    fn run(g: &mut Game, buttons: u8, ticks: u32) {
        for _ in 0..ticks {
            g.step(buttons);
        }
    }

    /// Press a button for a single tick, then release.
    fn tap(g: &mut Game, buttons: u8) {
        g.step(buttons);
        g.step(0);
    }

    #[test]
    fn starts_with_ball_in_hands_of_player_zero() {
        let g = Game::new(1);
        assert_eq!(g.ball.state, BallState::Held(0));
        assert_eq!(g.controlled, 0);
        assert_eq!(g.score, 0);
    }

    #[test]
    fn controlled_player_moves_with_input() {
        let mut g = Game::new(1);
        let x0 = g.players[0].x;
        run(&mut g, RIGHT, 60);
        assert!(g.players[0].x > x0 + 10.0);
    }

    #[test]
    fn pass_hands_control_to_receiver() {
        let mut g = Game::new(1);
        tap(&mut g, PASS | DOWN);
        assert!(matches!(g.ball.state, BallState::Pass { .. }));
        run(&mut g, 0, 120);
        match g.ball.state {
            BallState::Held(h) => assert_eq!(h, g.controlled),
            s => panic!("ball should have been caught, got {s:?}"),
        }
        assert_ne!(g.controlled, 0);
    }

    #[test]
    fn close_shots_eventually_score_and_possession_resets() {
        // Try many seeds from point-blank range; at least one must go in and bump the score.
        let mut scored = false;
        for seed in 1..40 {
            let mut g = Game::new(seed);
            g.players[0].x = HOOP_X - 4.0;
            g.players[0].y = HOOP_Y;
            g.ball.state = BallState::Held(0);
            tap(&mut g, SHOOT);
            run(&mut g, 0, 90);
            if g.score > 0 {
                assert!(g.score == 2 || g.score == 3);
                run(&mut g, 0, 150);
                assert_eq!(
                    g.ball.state,
                    BallState::Held(0),
                    "possession resets after a basket"
                );
                scored = true;
                break;
            }
        }
        assert!(scored);
    }

    #[test]
    fn long_range_is_worth_three() {
        for seed in 1..80 {
            let mut g = Game::new(seed);
            g.players[0].x = HOOP_X - 26.0;
            g.players[0].y = HOOP_Y;
            tap(&mut g, SHOOT);
            run(&mut g, 0, 120);
            if g.score > 0 {
                assert_eq!(g.score, 3);
                return;
            }
        }
        panic!("no three-pointer went in over 80 seeds");
    }

    #[test]
    fn missed_shot_can_be_recovered() {
        for seed in 1..200 {
            let mut g = Game::new(seed);
            g.players[0].x = HOOP_X - 30.0;
            g.players[0].y = HOOP_Y;
            tap(&mut g, SHOOT);
            run(&mut g, 0, 600);
            if g.score == 0 {
                assert!(
                    matches!(g.ball.state, BallState::Held(_)),
                    "a teammate should rebound a miss"
                );
                return;
            }
        }
        panic!("every shot went in?");
    }

    #[test]
    fn shot_clock_violation_resets_possession() {
        let mut g = Game::new(1);
        run(&mut g, 0, (SHOT_CLOCK_SECS * 60.0) as u32 + 5);
        assert!(g.shot_clock > 20.0);
        assert_eq!(g.score, 0);
    }

    #[test]
    fn game_ends_and_restarts() {
        let mut g = Game::new(1);
        // Hold the ball and let the clocks run out; shot clock resets keep play legal.
        run(&mut g, 0, (PERIOD_SECS * 60.0) as u32 + 10);
        assert_eq!(g.phase, Phase::Final);
        tap(&mut g, START);
        assert_eq!(g.phase, Phase::Playing);
        assert!(g.clock > PERIOD_SECS - 0.1);
    }

    #[test]
    fn simulation_is_deterministic() {
        let script = |g: &mut Game| {
            for t in 0..2000u32 {
                let b = match (t / 37) % 6 {
                    0 => RIGHT,
                    1 => RIGHT | DOWN,
                    2 => PASS,
                    3 => 0,
                    4 => SHOOT,
                    _ => DOWN,
                };
                g.step(b);
            }
        };
        let (mut a, mut b) = (Game::new(7), Game::new(7));
        script(&mut a);
        script(&mut b);
        assert_eq!(a.score, b.score);
        assert_eq!(a.ball.x.to_bits(), b.ball.x.to_bits());
        assert_eq!(a.players[0].y.to_bits(), b.players[0].y.to_bits());
    }

    #[test]
    fn view_cuts_with_hysteresis() {
        let mut g = Game::new(1);
        assert_eq!(g.view, View::Side);
        g.players[0].x = END_VIEW_ENTER_X + 1.0;
        g.step(0);
        assert_eq!(g.view, View::End);
        g.players[0].x = (END_VIEW_ENTER_X + END_VIEW_EXIT_X) / 2.0;
        g.step(0);
        assert_eq!(g.view, View::End, "no flicker between the two thresholds");
        g.players[0].x = END_VIEW_EXIT_X - 3.0; // the held ball sits 1.4 ft ahead
        g.step(0);
        assert_eq!(g.view, View::Side);
    }

    #[test]
    fn end_view_directions_are_relative_to_the_screen() {
        let mut g = Game::new(1);
        g.players[0].x = END_VIEW_ENTER_X + 2.0;
        g.players[0].y = 25.0;
        g.step(0);
        assert_eq!(g.view, View::End);
        let (x0, y0) = (g.players[0].x, g.players[0].y);
        run(&mut g, input::UP, 20);
        assert!(g.players[0].x > x0 + 1.0, "up runs toward the basket");
        let (x1, y1) = (g.players[0].x, g.players[0].y);
        assert!((y1 - y0).abs() < 0.01);
        run(&mut g, 0, 30);
        let y2 = g.players[0].y;
        run(&mut g, input::LEFT, 20);
        assert!(
            g.players[0].y > y2 + 1.0,
            "screen-left is +y in the end view"
        );
        assert!(g.players[0].x < x1 + 3.0);
    }

    #[test]
    fn held_direction_survives_the_camera_cut() {
        let mut g = Game::new(1);
        g.players[0].x = END_VIEW_ENTER_X - 1.0;
        g.players[0].y = 25.0;
        // Run right (+x) in the side view straight through the cut.
        run(&mut g, RIGHT, 40);
        assert_eq!(g.view, View::End);
        assert!(
            (g.players[0].y - 25.0).abs() < 0.5,
            "still running toward the basket"
        );
        // After releasing, the same key means "screen right" = -y.
        run(&mut g, 0, 40);
        let y = g.players[0].y;
        run(&mut g, RIGHT, 20);
        assert!(g.players[0].y < y - 1.0);
    }
}
