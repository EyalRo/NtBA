//! Deterministic two-team simulation: each human steers one player of a team of five.
//!
//! Only `+ - * /` and `sqrt` on `f32` are used so a future lock-step P2P build
//! stays bit-identical across machines.

use core::ops::Range;

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
pub const TEAM_SIZE: usize = 5;
/// Players 0..5 are home (attacking right), 5..10 away (attacking left).
pub const PLAYERS: usize = 2 * TEAM_SIZE;
const DIR_MASK: u8 = input::UP | input::DOWN | input::LEFT | input::RIGHT;
const SCORED_PAUSE: f32 = 1.8;
const MESSAGE_SECS: f32 = 1.6;
const STEAL_REACH: f32 = 3.0;
const STEAL_CHANCE: f32 = 0.3;
const BLOCK_REACH: f32 = 4.0;
const BLOCK_CHANCE: f32 = 0.5;
const BLOCK_WINDOW: f32 = 0.3;
const INTERCEPT_REACH: f32 = 1.2;
const INTERCEPT_DELAY: f32 = 0.1;
const CONTACT_REACH: f32 = 2.4;
/// Speed (ft/s) a defender must be running at the handler to strip the ball on contact.
const CONTACT_SPEED: f32 = 6.0;
/// No contact steals this long after the ball changes team.
const STEAL_LOCK_SECS: f32 = 0.75;

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

pub fn team_of(i: usize) -> usize {
    i / TEAM_SIZE
}

fn members(team: usize) -> Range<usize> {
    team * TEAM_SIZE..(team + 1) * TEAM_SIZE
}

/// +1 when the team attacks toward +x.
fn attack_dir(team: usize) -> f32 {
    if team == 0 {
        1.0
    } else {
        -1.0
    }
}

/// Mirror a home-team x so the away team uses the same spots toward its own basket.
fn team_x(team: usize, x: f32) -> f32 {
    if team == 0 {
        x
    } else {
        COURT_L - x
    }
}

/// X of the basket the team attacks.
pub fn hoop_x(team: usize) -> f32 {
    team_x(team, HOOP_X)
}

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
    Pass { to: usize, t: f32 },
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

/// One of the two baskets, named by the end of the court it hangs over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basket {
    Left,
    Right,
}

/// Which camera is showing the play. The simulation owns this because it changes what the
/// direction keys mean (on the end-on view "up" runs toward the basket).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// Sideline view that scrolls along the court (transition and backcourt play).
    Side,
    /// Looking at this basket from midcourt (half-court play).
    End(Basket),
}

/// Translate screen-relative direction bits into court axes (`UP` = -y, `RIGHT` = +x).
fn map_dir(bits: u8, view: View) -> u8 {
    // Court directions for screen up, down, left and right.
    let (up, down, left, right) = match view {
        View::Side => return bits,
        View::End(Basket::Right) => (input::RIGHT, input::LEFT, input::DOWN, input::UP),
        View::End(Basket::Left) => (input::LEFT, input::RIGHT, input::UP, input::DOWN),
    };
    let mut out = 0;
    for (key, court) in [
        (input::UP, up),
        (input::DOWN, down),
        (input::LEFT, left),
        (input::RIGHT, right),
    ] {
        if bits & key != 0 {
            out |= court;
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Playing,
    Final,
}

/// 64-bit FNV-1a, fed byte by byte so the result never depends on endianness or layout.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    fn u8(&mut self, b: u8) {
        self.0 = (self.0 ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn u32(&mut self, v: u32) {
        v.to_le_bytes().iter().for_each(|&b| self.u8(b));
    }

    fn u64(&mut self, v: u64) {
        v.to_le_bytes().iter().for_each(|&b| self.u8(b));
    }

    fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
}

pub struct Game {
    pub players: [Player; PLAYERS],
    pub ball: Ball,
    /// The player each human steers, indexed by team.
    pub controlled: [usize; 2],
    /// Team that has, or last had, the ball.
    pub offense: usize,
    pub score: [u32; 2],
    pub clock: f32,
    pub shot_clock: f32,
    pub phase: Phase,
    pub view: View,
    /// View whose key mapping stays in force for a direction held across a camera cut.
    legacy_view: [Option<View>; 2],
    pub message: Option<(&'static str, f32)>,
    pub ticks: u64,
    rng: u32,
    prev_input: [u8; 2],
    pending_points: u32,
    steal_lock: f32,
}

impl Game {
    pub fn new(seed: u32) -> Self {
        let mut g = Game {
            players: [Player::default(); PLAYERS],
            ball: Ball {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                vx: 0.0,
                vy: 0.0,
                vz: 0.0,
                state: BallState::Held(0),
            },
            controlled: [0, TEAM_SIZE],
            offense: 0,
            score: [0; 2],
            clock: PERIOD_SECS,
            shot_clock: SHOT_CLOCK_SECS,
            phase: Phase::Playing,
            view: View::Side,
            legacy_view: [None; 2],
            message: None,
            ticks: 0,
            rng: seed.wrapping_mul(0x9E37_79B1) | 1,
            prev_input: [0; 2],
            pending_points: 2,
            steal_lock: 0.0,
        };
        for _ in 0..8 {
            g.rand();
        }
        g.reset_possession(0);
        g
    }

    /// A 64-bit FNV-1a hash of every field that influences the simulation. Two peers that fed
    /// the same inputs to the same seed must report the same hash on every tick; it is how
    /// online play detects a desync. Floats are hashed by bit pattern.
    pub fn state_hash(&self) -> u64 {
        let mut h = Fnv::new();
        for p in &self.players {
            for v in [p.x, p.y, p.vx, p.vy, p.anim, p.jump] {
                h.f32(v);
            }
            h.u8(p.facing as u8);
        }
        let b = &self.ball;
        for v in [b.x, b.y, b.z, b.vx, b.vy, b.vz] {
            h.f32(v);
        }
        match b.state {
            BallState::Held(i) => {
                h.u8(0);
                h.u8(i as u8);
            }
            BallState::Pass { to, t } => {
                h.u8(1);
                h.u8(to as u8);
                h.f32(t);
            }
            BallState::Shot { t, dur, made } => {
                h.u8(2);
                h.f32(t);
                h.f32(dur);
                h.u8(made as u8);
            }
            BallState::Loose => h.u8(3),
            BallState::Scored { t } => {
                h.u8(4);
                h.f32(t);
            }
        }
        for c in self.controlled {
            h.u8(c as u8);
        }
        h.u8(self.offense as u8);
        for s in self.score {
            h.u32(s);
        }
        h.f32(self.clock);
        h.f32(self.shot_clock);
        h.u8(self.phase as u8);
        for v in std::iter::once(Some(self.view)).chain(self.legacy_view) {
            match v {
                None => h.u8(0),
                Some(View::Side) => h.u8(1),
                Some(View::End(Basket::Left)) => h.u8(2),
                Some(View::End(Basket::Right)) => h.u8(3),
            }
        }
        match self.message {
            None => h.u8(0),
            Some((text, left)) => {
                h.u8(1);
                text.bytes().for_each(|c| h.u8(c));
                h.f32(left);
            }
        }
        h.u64(self.ticks);
        h.u32(self.rng);
        self.prev_input.iter().for_each(|&i| h.u8(i));
        h.u32(self.pending_points);
        h.f32(self.steal_lock);
        h.0
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / 16_777_216.0
    }

    /// Line both teams up with the ball in the hands of `team`'s first player.
    pub fn reset_possession(&mut self, team: usize) {
        let facing = attack_dir(team) as i8;
        for (i, &(x, y)) in members(team).zip(START_SPOTS.iter()) {
            self.players[i] = Player {
                x: team_x(team, x),
                y,
                facing,
                ..Player::default()
            };
        }
        let handler = members(team).start;
        self.ball.state = BallState::Held(handler);
        for i in members(1 - team) {
            let (x, y) = self.guard_spot(i);
            self.players[i] = Player {
                x,
                y,
                facing: -facing,
                ..Player::default()
            };
        }
        self.offense = team;
        self.controlled = [0, TEAM_SIZE];
        self.view = View::Side;
        self.legacy_view = [None; 2];
        self.shot_clock = SHOT_CLOCK_SECS;
        self.steal_lock = STEAL_LOCK_SECS;
        self.ball.vx = 0.0;
        self.ball.vy = 0.0;
        self.ball.vz = 0.0;
        self.sync_held_ball();
    }

    fn restart(&mut self) {
        self.score = [0; 2];
        self.clock = PERIOD_SECS;
        self.phase = Phase::Playing;
        self.message = None;
        self.reset_possession(0);
    }

    fn say(&mut self, text: &'static str) {
        self.message = Some((text, MESSAGE_SECS));
    }

    /// Advance the simulation by one fixed tick; `pads` holds each team's input bitmask.
    pub fn step(&mut self, pads: [u8; 2]) {
        let pressed = [0, 1].map(|t| pads[t] & !self.prev_input[t]);
        self.prev_input = pads;
        self.ticks += 1;

        if let Some((text, left)) = self.message {
            self.message = if left > DT {
                Some((text, left - DT))
            } else {
                None
            };
        }

        if self.phase == Phase::Final {
            if (pressed[0] | pressed[1]) & START != 0 {
                self.restart();
            }
            return;
        }

        self.update_clocks();
        let moves = [0, 1].map(|t| self.remap_buttons(t, pads[t]));
        for team in 0..2 {
            self.handle_actions(team, pressed[team], moves[team]);
        }
        self.move_players(moves);
        self.contact_steal();
        self.update_ball();
        self.update_view(pads);
    }

    /// Apply the current camera's direction mapping to one pad's raw key bitmask.
    fn remap_buttons(&mut self, pad: usize, raw: u8) -> u8 {
        let dirs = raw & DIR_MASK;
        if dirs == 0 {
            self.legacy_view[pad] = None;
        }
        let view = self.legacy_view[pad].unwrap_or(self.view);
        (raw & !DIR_MASK) | map_dir(dirs, view)
    }

    /// Cut between the side and end-on cameras based on where the ball is.
    fn update_view(&mut self, pads: [u8; 2]) {
        let x = self.ball.x;
        let next = match self.view {
            View::Side if x >= END_VIEW_ENTER_X => View::End(Basket::Right),
            View::Side if x <= COURT_L - END_VIEW_ENTER_X => View::End(Basket::Left),
            View::End(Basket::Right) if x <= END_VIEW_EXIT_X => View::Side,
            View::End(Basket::Left) if x >= COURT_L - END_VIEW_EXIT_X => View::Side,
            v => v,
        };
        if next != self.view {
            // Keep a held direction meaning what it meant until the player lets go.
            for (pad, raw) in pads.iter().enumerate() {
                if raw & DIR_MASK != 0 {
                    self.legacy_view[pad] = Some(self.view);
                }
            }
            self.view = next;
        }
    }

    fn update_clocks(&mut self) {
        self.steal_lock = (self.steal_lock - DT).max(0.0);
        self.clock -= DT;
        if self.clock <= 0.0 {
            self.clock = 0.0;
            self.phase = Phase::Final;
            return;
        }
        if !matches!(self.ball.state, BallState::Scored { .. }) {
            self.shot_clock -= DT;
            if self.shot_clock <= 0.0 {
                self.reset_possession(1 - self.offense);
                self.say("SHOT CLOCK");
            }
        }
    }

    fn handle_actions(&mut self, team: usize, pressed: u8, buttons: u8) {
        let ours = |h: usize| team_of(h) == team;
        let me = self.controlled[team];
        let state = self.ball.state;
        let in_hand =
            matches!(state, BallState::Held(h) | BallState::Pass { to: h, .. } if ours(h));
        if pressed & SWITCH != 0 && !in_hand {
            self.controlled[team] =
                self.nearest_in(members(team), self.ball.x, self.ball.y, Some(me));
        }
        match state {
            BallState::Held(h) if ours(h) => {
                if pressed & SHOOT != 0 {
                    self.shoot(h);
                } else if pressed & PASS != 0 {
                    self.pass(h, buttons);
                }
            }
            BallState::Held(_) if pressed & SHOOT != 0 => self.defend(me),
            BallState::Shot { .. } if team != self.offense && pressed & SHOOT != 0 => {
                self.defend(me)
            }
            _ => {}
        }
    }

    fn nearest_in(&self, range: Range<usize>, x: f32, y: f32, skip: Option<usize>) -> usize {
        let mut best = range.start;
        let mut best_d = f32::MAX;
        for i in range {
            if Some(i) == skip {
                continue;
            }
            let p = &self.players[i];
            let d = dist2(p.x, p.y, x, y);
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        best
    }

    /// Put the ball in `i`'s hands; a change of team resets the shot clock.
    fn gain(&mut self, i: usize) {
        let team = team_of(i);
        if team != self.offense {
            self.offense = team;
            self.shot_clock = SHOT_CLOCK_SECS;
            self.steal_lock = STEAL_LOCK_SECS;
        }
        self.ball.state = BallState::Held(i);
        self.controlled[team] = i;
        self.sync_held_ball();
    }

    fn shoot(&mut self, h: usize) {
        let team = team_of(h);
        let hx = hoop_x(team);
        let p = self.players[h];
        let d = dist(p.x, p.y, hx, HOOP_Y);
        let chance = (0.95 - 0.022 * d).clamp(0.12, 0.9);
        let made = self.rand() < chance;
        let dur = 0.55 + d * 0.012;
        let (tx, ty) = if made {
            (hx, HOOP_Y)
        } else {
            let ox = (self.rand() - 0.5) * 3.0;
            let oy = (self.rand() - 0.5) * 3.0;
            (hx + ox, HOOP_Y + oy)
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
        self.players[h].facing = attack_dir(team) as i8;
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
        for i in members(team_of(h)) {
            if i == h {
                continue;
            }
            let p = &self.players[i];
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
            self.ball.state = BallState::Pass { to, t: 0.0 };
            self.controlled[team_of(h)] = to;
        }
    }

    /// Defensive jump: may steal a held ball or block a fresh shot within reach.
    fn defend(&mut self, me: usize) {
        if self.players[me].jump > 0.0 {
            return;
        }
        self.players[me].jump = 0.5;
        let p = self.players[me];
        let d = dist(p.x, p.y, self.ball.x, self.ball.y);
        let state = self.ball.state;
        match state {
            BallState::Held(_) if d < STEAL_REACH && self.rand() < STEAL_CHANCE => {
                self.gain(me);
                self.say("STEAL!");
            }
            BallState::Shot { t, .. }
                if t < BLOCK_WINDOW && d < BLOCK_REACH && self.rand() < BLOCK_CHANCE =>
            {
                let back = -attack_dir(self.offense);
                self.ball.vx = back * (6.0 + self.rand() * 6.0);
                self.ball.vy = (self.rand() - 0.5) * 10.0;
                self.ball.vz = 4.0;
                self.ball.state = BallState::Loose;
                self.say("BLOCKED!");
            }
            _ => {}
        }
    }

    /// A human defender running into the ball handler takes the ball.
    fn contact_steal(&mut self) {
        let BallState::Held(h) = self.ball.state else {
            return;
        };
        if self.steal_lock > 0.0 {
            return;
        }
        let d = self.controlled[1 - team_of(h)];
        let (p, q) = (self.players[d], self.players[h]);
        let (dx, dy) = (q.x - p.x, q.y - p.y);
        let gap = (dx * dx + dy * dy).sqrt().max(0.001);
        if gap < CONTACT_REACH && (p.vx * dx + p.vy * dy) / gap > CONTACT_SPEED {
            self.gain(d);
            self.say("STEAL!");
        }
    }

    /// Where AI defender `i` stands: between its man and the basket that man attacks.
    fn guard_spot(&self, i: usize) -> (f32, f32) {
        let man = (i + TEAM_SIZE) % PLAYERS;
        let m = self.players[man];
        let (dx, dy) = (hoop_x(team_of(man)) - m.x, HOOP_Y - m.y);
        let d = (dx * dx + dy * dy).sqrt().max(0.001);
        let gap = if self.ball.state == BallState::Held(man) {
            3.0
        } else {
            5.0
        };
        let gap = f32::min(gap, d * 0.5);
        (m.x + dx / d * gap, m.y + dy / d * gap)
    }

    fn move_players(&mut self, pads: [u8; 2]) {
        let handler = match self.ball.state {
            BallState::Held(h) => Some(h),
            _ => None,
        };
        let chasers = if self.ball.state == BallState::Loose {
            [0, 1].map(|t| Some(self.nearest_in(members(t), self.ball.x, self.ball.y, None)))
        } else {
            [None; 2]
        };

        for i in 0..PLAYERS {
            let team = team_of(i);
            let (mut tx, mut ty);
            if i == self.controlled[team] {
                let (ix, iy) = input::dir(pads[team]);
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
                let (gx, gy, speed) = if chasers[team] == Some(i) {
                    (self.ball.x, self.ball.y, 20.0)
                } else if team == self.offense {
                    let (x, y) = OFFENSE_SPOTS[i % TEAM_SIZE];
                    (team_x(team, x), y, 15.0)
                } else {
                    let (x, y) = self.guard_spot(i);
                    (x, y, 16.0)
                };
                let p = &self.players[i];
                let (dx, dy) = (gx - p.x, gy - p.y);
                let d = (dx * dx + dy * dy).sqrt();
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

    /// Keep players from stacking on the same spot.
    fn separate_players(&mut self) {
        for i in 0..PLAYERS {
            for j in (i + 1)..PLAYERS {
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
            BallState::Pass { to, t } => self.update_pass(to, t),
            BallState::Shot { t, dur, made } => self.update_shot(t, dur, made),
            BallState::Loose => self.update_loose(),
            BallState::Scored { t } => self.update_scored(t),
        }
    }

    fn update_pass(&mut self, to: usize, t: f32) {
        if t >= INTERCEPT_DELAY {
            let (bx, by) = (self.ball.x, self.ball.y);
            let thief = members(1 - team_of(to)).find(|&i| {
                let p = &self.players[i];
                dist(p.x, p.y, bx, by) < INTERCEPT_REACH
            });
            if let Some(i) = thief {
                self.gain(i);
                self.say("STEAL!");
                return;
            }
        }
        let r = self.players[to];
        let (tx, ty) = (r.x + r.vx * 0.15, r.y + r.vy * 0.15);
        let (dx, dy) = (tx - self.ball.x, ty - self.ball.y);
        let d = (dx * dx + dy * dy).sqrt();
        let step = 55.0 * DT;
        if d <= step.max(1.8) {
            self.gain(to);
            return;
        }
        self.ball.x += dx / d * step;
        self.ball.y += dy / d * step;
        self.ball.z = approach(self.ball.z, 4.0, 12.0 * DT);
        self.ball.state = BallState::Pass { to, t: t + DT };
    }

    fn update_shot(&mut self, t: f32, dur: f32, made: bool) {
        self.integrate_ball();
        let t = t + DT;
        if t < dur {
            self.ball.state = BallState::Shot { t, dur, made };
            return;
        }
        let team = self.offense;
        if made {
            self.ball.x = hoop_x(team);
            self.ball.y = HOOP_Y;
            self.ball.z = RIM_Z;
            self.ball.vx = 0.0;
            self.ball.vy = 0.0;
            self.ball.vz = -6.0;
            self.score[team] += self.pending_points;
            self.say(if self.pending_points == 3 {
                "THREE POINTS!"
            } else {
                "SCORE!"
            });
            self.ball.state = BallState::Scored { t: 0.0 };
        } else {
            // Rim it out: kick back toward the court with a bit of randomness.
            let back = -attack_dir(team);
            self.ball.z = RIM_Z;
            self.ball.vx = back * (3.0 + self.rand() * 6.0);
            self.ball.vy = (self.rand() - 0.5) * 8.0;
            self.ball.vz = 12.0;
            self.ball.state = BallState::Loose;
            self.shot_clock = SHOT_CLOCK_SECS;
            self.say("MISS");
            let (rx, ry) = (self.ball.x + back * 10.0, self.ball.y);
            self.controlled = [0, 1].map(|t| self.nearest_in(members(t), rx, ry, None));
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
            let i = self.nearest_in(0..PLAYERS, self.ball.x, self.ball.y, None);
            let p = self.players[i];
            if dist(p.x, p.y, self.ball.x, self.ball.y) < 2.6 {
                self.gain(i);
            }
        }
    }

    fn update_scored(&mut self, t: f32) {
        self.integrate_ball();
        let t = t + DT;
        if t >= SCORED_PAUSE {
            self.reset_possession(1 - self.offense);
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
    use crate::input::{DOWN, LEFT, RIGHT, UP};

    /// Run home's pad only.
    fn run(g: &mut Game, buttons: u8, ticks: u32) {
        run2(g, [buttons, 0], ticks);
    }

    fn run2(g: &mut Game, pads: [u8; 2], ticks: u32) {
        for _ in 0..ticks {
            g.step(pads);
        }
    }

    /// Press a button on home's pad for a single tick, then release.
    fn tap(g: &mut Game, buttons: u8) {
        g.step([buttons, 0]);
        g.step([0, 0]);
    }

    #[test]
    fn starts_with_ball_in_hands_of_player_zero() {
        let g = Game::new(1);
        assert_eq!(g.ball.state, BallState::Held(0));
        assert_eq!(g.controlled, [0, TEAM_SIZE]);
        assert_eq!(g.offense, 0);
        assert_eq!(g.score, [0, 0]);
    }

    #[test]
    fn controlled_player_moves_with_input() {
        let mut g = Game::new(1);
        g.players[TEAM_SIZE].y = 5.0; // step the on-ball defender aside
        let x0 = g.players[0].x;
        run(&mut g, RIGHT, 60);
        assert!(g.players[0].x > x0 + 10.0);
    }

    #[test]
    fn each_pad_steers_its_own_team() {
        let mut g = Game::new(1);
        let (home, away) = (g.controlled[0], g.controlled[1]);
        let (hx, ay) = (g.players[home].x, g.players[away].y);
        run2(&mut g, [0, DOWN], 60);
        assert!(g.players[away].y > ay + 10.0);
        assert!((g.players[home].x - hx).abs() < 0.5);
    }

    #[test]
    fn pass_hands_control_to_receiver() {
        let mut g = Game::new(1);
        tap(&mut g, PASS | DOWN);
        assert!(matches!(g.ball.state, BallState::Pass { .. }));
        run(&mut g, 0, 120);
        match g.ball.state {
            BallState::Held(h) => assert_eq!(h, g.controlled[0]),
            s => panic!("ball should have been caught, got {s:?}"),
        }
        assert_ne!(g.controlled[0], 0);
    }

    #[test]
    fn close_shots_eventually_score_and_the_other_team_inbounds() {
        // Try many seeds from point-blank range; at least one must go in and bump the score.
        let mut scored = false;
        for seed in 1..40 {
            let mut g = Game::new(seed);
            g.players[0].x = HOOP_X - 4.0;
            g.players[0].y = HOOP_Y;
            g.ball.state = BallState::Held(0);
            tap(&mut g, SHOOT);
            run(&mut g, 0, 90);
            if g.score[0] > 0 {
                assert!(g.score[0] == 2 || g.score[0] == 3);
                run(&mut g, 0, 150);
                assert_eq!(g.ball.state, BallState::Held(TEAM_SIZE));
                assert_eq!(g.offense, 1);
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
            if g.score[0] > 0 {
                assert_eq!(g.score[0], 3);
                return;
            }
        }
        panic!("no three-pointer went in over 80 seeds");
    }

    #[test]
    fn away_team_scores_on_the_left_basket() {
        for seed in 1..40 {
            let mut g = Game::new(seed);
            g.reset_possession(1);
            let h = g.controlled[1];
            g.players[h].x = COURT_L - HOOP_X + 4.0;
            g.players[h].y = HOOP_Y;
            g.step([0, SHOOT]);
            run(&mut g, 0, 90);
            if g.score[1] > 0 {
                assert_eq!(g.score[0], 0);
                run(&mut g, 0, 150);
                assert_eq!(g.ball.state, BallState::Held(0));
                assert_eq!(g.offense, 0);
                return;
            }
        }
        panic!("the away team never scored");
    }

    #[test]
    fn missed_shot_can_be_recovered() {
        for seed in 1..200 {
            let mut g = Game::new(seed);
            g.players[0].x = HOOP_X - 30.0;
            g.players[0].y = HOOP_Y;
            tap(&mut g, SHOOT);
            run(&mut g, 0, 600);
            if g.score[0] == 0 {
                assert!(
                    matches!(g.ball.state, BallState::Held(_)),
                    "someone should rebound a miss"
                );
                return;
            }
        }
        panic!("every shot went in?");
    }

    #[test]
    fn defender_can_steal_the_ball() {
        for seed in 1..40 {
            let mut g = Game::new(seed);
            let d = g.controlled[1];
            g.players[d].x = g.ball.x + 1.0;
            g.players[d].y = g.ball.y;
            g.step([0, SHOOT]);
            if g.ball.state == BallState::Held(d) {
                assert_eq!(g.offense, 1);
                assert_eq!(g.shot_clock, SHOT_CLOCK_SECS);
                return;
            }
        }
        panic!("no steal in 40 seeds");
    }

    #[test]
    fn running_into_the_handler_steals_the_ball() {
        let mut g = Game::new(1);
        let d = g.controlled[1];
        run2(&mut g, [0, LEFT], 90);
        assert_eq!(g.ball.state, BallState::Held(d));
        assert_eq!(g.offense, 1);
    }

    #[test]
    fn handler_running_into_a_still_defender_keeps_the_ball() {
        let mut g = Game::new(1);
        run(&mut g, RIGHT, 120);
        assert_eq!(g.ball.state, BallState::Held(0));
    }

    #[test]
    fn steal_attempt_out_of_reach_fails() {
        let mut g = Game::new(1);
        let d = g.controlled[1];
        g.players[d].x = g.ball.x + 8.0;
        g.step([0, SHOOT]);
        assert_eq!(g.ball.state, BallState::Held(0));
        assert!(g.players[d].jump > 0.0);
    }

    #[test]
    fn defender_in_the_lane_intercepts_a_pass() {
        let mut g = Game::new(1);
        for i in 2..PLAYERS {
            g.players[i].x = 90.0;
            g.players[i].y = 2.0 + i as f32 * 4.0;
        }
        g.players[1].x = 46.0;
        g.players[1].y = 25.0;
        g.players[6].x = 40.0;
        g.players[6].y = 25.3;
        tap(&mut g, PASS);
        run(&mut g, 0, 30);
        assert_eq!(g.ball.state, BallState::Held(6));
        assert_eq!(g.offense, 1);
        assert_eq!(g.controlled[1], 6);
    }

    #[test]
    fn shot_clock_violation_turns_the_ball_over() {
        let mut g = Game::new(1);
        run(&mut g, 0, (SHOT_CLOCK_SECS * 60.0) as u32 + 5);
        assert!(g.shot_clock > 20.0);
        assert_eq!(g.offense, 1);
        assert_eq!(g.ball.state, BallState::Held(TEAM_SIZE));
        assert_eq!(g.score, [0, 0]);
    }

    #[test]
    fn game_ends_and_restarts() {
        let mut g = Game::new(1);
        // Hold the ball and let the clocks run out; shot clock turnovers keep play legal.
        run(&mut g, 0, (PERIOD_SECS * 60.0) as u32 + 10);
        assert_eq!(g.phase, Phase::Final);
        g.step([0, START]);
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
                let a = match (t / 23) % 4 {
                    0 => LEFT,
                    1 => SHOOT,
                    2 => UP | SWITCH,
                    _ => PASS,
                };
                g.step([b, a]);
            }
        };
        let (mut a, mut b) = (Game::new(7), Game::new(7));
        script(&mut a);
        script(&mut b);
        assert_eq!(a.score, b.score);
        assert_eq!(a.ball.x.to_bits(), b.ball.x.to_bits());
        assert_eq!(a.players[0].y.to_bits(), b.players[0].y.to_bits());
        assert_eq!(a.players[7].x.to_bits(), b.players[7].x.to_bits());
    }

    #[test]
    fn view_cuts_with_hysteresis() {
        let mut g = Game::new(1);
        assert_eq!(g.view, View::Side);
        g.players[0].x = END_VIEW_ENTER_X + 1.0;
        g.step([0, 0]);
        assert_eq!(g.view, View::End(Basket::Right));
        g.players[0].x = (END_VIEW_ENTER_X + END_VIEW_EXIT_X) / 2.0;
        g.step([0, 0]);
        assert_eq!(
            g.view,
            View::End(Basket::Right),
            "no flicker between the two thresholds"
        );
        g.players[0].x = END_VIEW_EXIT_X - 3.0; // the held ball sits 1.4 ft ahead
        g.step([0, 0]);
        assert_eq!(g.view, View::Side);
    }

    #[test]
    fn end_view_directions_are_relative_to_the_screen() {
        let mut g = Game::new(1);
        g.players[0].x = END_VIEW_ENTER_X + 2.0;
        g.players[0].y = 25.0;
        g.step([0, 0]);
        assert_eq!(g.view, View::End(Basket::Right));
        let (x0, y0) = (g.players[0].x, g.players[0].y);
        run(&mut g, UP, 20);
        assert!(g.players[0].x > x0 + 1.0, "up runs toward the basket");
        let (x1, y1) = (g.players[0].x, g.players[0].y);
        assert!((y1 - y0).abs() < 0.01);
        run(&mut g, 0, 30);
        let y2 = g.players[0].y;
        run(&mut g, LEFT, 20);
        assert!(
            g.players[0].y > y2 + 1.0,
            "screen-left is +y in the end view"
        );
        assert!(g.players[0].x < x1 + 3.0);
    }

    #[test]
    fn left_end_view_mirrors_the_directions() {
        let mut g = Game::new(1);
        g.reset_possession(1);
        let h = g.controlled[1];
        g.players[h].x = COURT_L - END_VIEW_ENTER_X - 2.0;
        g.players[h].y = 25.0;
        g.step([0, 0]);
        assert_eq!(g.view, View::End(Basket::Left));
        let x0 = g.players[h].x;
        run2(&mut g, [0, UP], 20);
        assert!(g.players[h].x < x0 - 1.0, "up runs toward the left basket");
        run2(&mut g, [0, 0], 30);
        let y = g.players[h].y;
        run2(&mut g, [0, LEFT], 20);
        assert!(g.players[h].y < y - 1.0, "screen-left is -y here");
    }

    #[test]
    fn held_direction_survives_the_camera_cut() {
        let mut g = Game::new(1);
        g.players[0].x = END_VIEW_ENTER_X - 1.0;
        g.players[0].y = 25.0;
        // Run right (+x) in the side view straight through the cut.
        run(&mut g, RIGHT, 40);
        assert_eq!(g.view, View::End(Basket::Right));
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
