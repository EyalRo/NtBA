//! Computer opponent: steers one team's controlled player through the same pad a human uses.

use crate::game::{
    attack_dir, hoop_x, members, screen_dir, team_of, BallState, Game, Phase, Player, BLOCK_REACH,
    BLOCK_WINDOW, HOOP_Y, STEAL_REACH, THREE_POINT_DIST,
};
use crate::input::{DOWN, LEFT, PASS, RIGHT, SHOOT, SWITCH, UP};
use crate::TICK_HZ;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Novice,
    Pro,
    AllStar,
}

impl Level {
    /// 0 = Novice, 1 = Pro, 2 = All-Star.
    pub fn from_index(i: u32) -> Option<Level> {
        match i {
            0 => Some(Level::Novice),
            1 => Some(Level::Pro),
            2 => Some(Level::AllStar),
            _ => None,
        }
    }
}

/// How well a level plays. Rates are per second and scaled by how often it thinks.
struct Skill {
    /// Ticks between decisions: the reaction time.
    think: u32,
    /// Chance a decision is skipped and the old plan kept.
    hesitate: f32,
    /// Longest two it takes when open.
    range: f32,
    /// Open threes taken per second.
    three: f32,
    /// Gap kept between the ball handler and the basket.
    gap: f32,
    /// Steal swipes per second with the ball in reach.
    swipe: f32,
    /// Runs through the handler per second when close.
    bump: f32,
    /// Chance of jumping at a shot in reach.
    block: f32,
    /// Passes out of pressure per second.
    pass: f32,
}

fn skill(level: Level) -> Skill {
    match level {
        Level::Novice => Skill {
            think: 24,
            hesitate: 0.35,
            range: 22.0,
            three: 0.0,
            gap: 5.0,
            swipe: 0.15,
            bump: 0.0,
            block: 0.3,
            pass: 0.4,
        },
        Level::Pro => Skill {
            think: 10,
            hesitate: 0.1,
            range: 15.0,
            three: 0.3,
            gap: 3.0,
            swipe: 0.4,
            bump: 0.2,
            block: 0.5,
            pass: 1.5,
        },
        Level::AllStar => Skill {
            think: 4,
            hesitate: 0.0,
            range: 13.0,
            three: 0.6,
            gap: 2.5,
            swipe: 0.7,
            bump: 0.5,
            block: 0.8,
            pass: 4.0,
        },
    }
}

/// Defenders closer than this count as pressure on the ball handler.
const PRESSURE: f32 = 4.0;
/// A shooter needs this much room to take a two.
const OPEN: f32 = 4.5;
/// Point-blank range: always shoot.
const LAYUP: f32 = 7.0;

#[derive(Default)]
struct Plan {
    target: Option<(f32, f32)>,
    press: u8,
    /// Court direction held on this tick instead of steering (aims a pass).
    aim: Option<u8>,
}

impl Plan {
    fn go(x: f32, y: f32) -> Plan {
        Plan {
            target: Some((x, y)),
            ..Plan::default()
        }
    }

    fn press(mut self, button: u8) -> Plan {
        self.press |= button;
        self
    }
}

pub struct Cpu {
    team: usize,
    skill: Skill,
    rng: u32,
    wait: u32,
    target: Option<(f32, f32)>,
    /// Side of the defender the handler is driving past.
    lane: Option<f32>,
    /// Running through the handler on purpose to strip the ball.
    charging: bool,
}

impl Cpu {
    pub fn new(team: usize, level: Level, seed: u32) -> Self {
        Cpu {
            team,
            skill: skill(level),
            rng: seed.wrapping_mul(0x85EB_CA6B) | 1,
            wait: 0,
            target: None,
            lane: None,
            charging: false,
        }
    }

    pub fn team(&self) -> usize {
        self.team
    }

    /// The pad byte for this tick, in screen directions like a human's keys.
    pub fn pad(&mut self, g: &Game) -> u8 {
        if g.phase == Phase::Final {
            return 0;
        }
        let mut plan = Plan::default();
        if self.wait == 0 {
            self.wait = self.skill.think;
            if self.rand() >= self.skill.hesitate {
                self.charging = false;
                plan = self.decide(g);
                self.target = plan.target;
            }
        }
        self.wait -= 1;
        let me = g.players[g.controlled[self.team]];
        let mut court = plan
            .aim
            .unwrap_or_else(|| self.target.map_or(0, |(x, y)| steer(me.x, me.y, x, y, 1.5)));
        if !self.charging && toward_handler(g, self.team, &me, court) {
            court = 0;
        }
        plan.press | screen_dir(court, g.input_view(self.team))
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / 16_777_216.0
    }

    /// True with probability `per_sec` spread over the decisions made in a second.
    fn chance(&mut self, per_sec: f32) -> bool {
        self.rand() < per_sec * self.skill.think as f32 / TICK_HZ as f32
    }

    fn decide(&mut self, g: &Game) -> Plan {
        let me = g.controlled[self.team];
        match g.ball.state {
            BallState::Held(h) if team_of(h) == self.team => self.attack(g, h),
            BallState::Held(h) => self.guard(g, me, h),
            BallState::Pass { to, .. } if team_of(to) == self.team => Plan::go(g.ball.x, g.ball.y),
            BallState::Pass { to, .. } => {
                let r = g.players[to];
                Plan::go((g.ball.x + r.x) / 2.0, (g.ball.y + r.y) / 2.0)
            }
            BallState::Shot { t, .. } => self.contest(g, me, t),
            BallState::Loose => self.chase(g, me),
            BallState::Scored { .. } => Plan::default(),
        }
    }

    fn attack(&mut self, g: &Game, h: usize) -> Plan {
        let team = self.team;
        let p = g.players[h];
        let hx = hoop_x(team);
        let to_hoop = dist(p.x, p.y, hx, HOOP_Y);
        let (def, gap) = nearest(g, members(1 - team), p.x, p.y, None);
        let d = g.players[def];
        let in_front = (d.x - p.x) * (hx - p.x) + (d.y - p.y) * (HOOP_Y - p.y) > 0.0;

        let contested = gap < OPEN && in_front;
        let open_three = (THREE_POINT_DIST..THREE_POINT_DIST + 2.5).contains(&to_hoop)
            && gap > 7.0
            && self.chance(self.skill.three);
        if to_hoop < LAYUP
            || g.shot_clock < 2.5
            || (to_hoop <= self.skill.range && (!contested || to_hoop < self.skill.range * 0.6))
            || open_three
        {
            return Plan::go(p.x, p.y).press(SHOOT);
        }
        if gap < PRESSURE && in_front && self.chance(self.skill.pass) {
            if let Some(m) = open_mate(g, h) {
                let r = g.players[m];
                return Plan {
                    target: Some((r.x, r.y)),
                    press: PASS,
                    aim: Some(steer(p.x, p.y, r.x, r.y, 0.0)),
                };
            }
        }
        if gap < 8.0 && in_front {
            let side = if p.y >= d.y { 10.0 } else { -10.0 };
            self.lane.get_or_insert(side);
        } else {
            self.lane = None;
        }
        Plan::go(
            hx - attack_dir(team) * 5.0,
            HOOP_Y + self.lane.unwrap_or(0.0),
        )
    }

    fn guard(&mut self, g: &Game, me: usize, h: usize) -> Plan {
        let p = g.players[me];
        let q = g.players[h];
        let mine = dist(p.x, p.y, q.x, q.y);
        let (mate, theirs) = nearest(g, members(self.team), q.x, q.y, Some(me));
        if mine > 8.0 && theirs + 6.0 < mine {
            let m = g.players[mate];
            return Plan::go(m.x, m.y).press(SWITCH);
        }
        if p.jump == 0.0
            && dist(p.x, p.y, g.ball.x, g.ball.y) < STEAL_REACH - 0.4
            && self.chance(self.skill.swipe)
        {
            return Plan::go(q.x, q.y).press(SHOOT);
        }
        if mine < 6.0 && self.chance(self.skill.bump) {
            self.charging = true;
            let k = 3.0 / mine.max(0.001);
            return Plan::go(q.x + (q.x - p.x) * k, q.y + (q.y - p.y) * k);
        }
        let (dx, dy) = (hoop_x(team_of(h)) - q.x, HOOP_Y - q.y);
        let d = (dx * dx + dy * dy).sqrt().max(0.001);
        let gap = f32::min(self.skill.gap, d * 0.5);
        Plan::go(q.x + dx / d * gap, q.y + dy / d * gap)
    }

    fn contest(&mut self, g: &Game, me: usize, t: f32) -> Plan {
        let p = g.players[me];
        let shooter = g.offense;
        let plan = Plan::go(hoop_x(shooter) - attack_dir(shooter) * 7.0, HOOP_Y);
        if shooter != self.team
            && t < BLOCK_WINDOW
            && p.jump == 0.0
            && dist(p.x, p.y, g.ball.x, g.ball.y) < BLOCK_REACH
            && self.rand() < self.skill.block
        {
            return plan.press(SHOOT);
        }
        plan
    }

    fn chase(&mut self, g: &Game, me: usize) -> Plan {
        let (bx, by) = (g.ball.x, g.ball.y);
        let p = g.players[me];
        let (_, theirs) = nearest(g, members(self.team), bx, by, Some(me));
        let plan = Plan::go(bx, by);
        if theirs + 3.0 < dist(p.x, p.y, bx, by) {
            return plan.press(SWITCH);
        }
        plan
    }
}

/// True when stepping `court` would close on an opposing handler fast enough to strip by accident.
fn toward_handler(g: &Game, team: usize, me: &Player, court: u8) -> bool {
    let BallState::Held(h) = g.ball.state else {
        return false;
    };
    if team_of(h) == team {
        return false;
    }
    let q = g.players[h];
    let (dx, dy) = (q.x - me.x, q.y - me.y);
    if dx * dx + dy * dy > 5.0 * 5.0 {
        return false;
    }
    let (sx, sy) = crate::input::dir(court);
    sx * dx + sy * dy > 0.0
}

/// The teammate of `h` with the most room and a clear passing lane, if any.
fn open_mate(g: &Game, h: usize) -> Option<usize> {
    let team = team_of(h);
    let p = g.players[h];
    let hx = hoop_x(team);
    let mut best = None;
    let mut best_score = f32::MIN;
    for i in members(team) {
        if i == h {
            continue;
        }
        let r = g.players[i];
        let (_, room) = nearest(g, members(1 - team), r.x, r.y, None);
        let lane_clear = members(1 - team).all(|j| {
            let o = &g.players[j];
            seg_dist(o, &p, &r) > 2.0
        });
        if room < 4.0 || !lane_clear {
            continue;
        }
        let score = room - 0.3 * dist(r.x, r.y, hx, HOOP_Y);
        if score > best_score {
            best_score = score;
            best = Some(i);
        }
    }
    best
}

/// Nearest player in `range` to (x, y) and its distance.
fn nearest(
    g: &Game,
    range: core::ops::Range<usize>,
    x: f32,
    y: f32,
    skip: Option<usize>,
) -> (usize, f32) {
    let mut best = (range.start, f32::MAX);
    for i in range {
        if Some(i) == skip {
            continue;
        }
        let p = &g.players[i];
        let d = dist(p.x, p.y, x, y);
        if d < best.1 {
            best = (i, d);
        }
    }
    best
}

/// Court direction bits (8-way) from (x, y) toward (tx, ty); none within `stop` feet.
fn steer(x: f32, y: f32, tx: f32, ty: f32, stop: f32) -> u8 {
    let (dx, dy) = (tx - x, ty - y);
    let d = (dx * dx + dy * dy).sqrt();
    if d <= stop || d == 0.0 {
        return 0;
    }
    // sin(22.5 degrees): the edge between a straight and a diagonal direction.
    let edge = 0.38 * d;
    let mut bits = 0;
    if dx > edge {
        bits |= RIGHT;
    } else if dx < -edge {
        bits |= LEFT;
    }
    if dy > edge {
        bits |= DOWN;
    } else if dy < -edge {
        bits |= UP;
    }
    bits
}

/// Distance from `o` to the segment `a`-`b`.
fn seg_dist(o: &Player, a: &Player, b: &Player) -> f32 {
    let (vx, vy) = (b.x - a.x, b.y - a.y);
    let len2 = (vx * vx + vy * vy).max(0.001);
    let t = (((o.x - a.x) * vx + (o.y - a.y) * vy) / len2).clamp(0.0, 1.0);
    dist(o.x, o.y, a.x + vx * t, a.y + vy * t)
}

fn dist(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (ax - bx, ay - by);
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::PERIOD_SECS;

    const GAME_TICKS: u32 = (PERIOD_SECS * TICK_HZ as f32) as u32;

    /// Play a full game; a `None` side stands still.
    fn play(levels: [Option<Level>; 2], seed: u32) -> [u32; 2] {
        let mut g = Game::new(seed);
        let mut cpus = [0, 1].map(|t| levels[t].map(|l| Cpu::new(t, l, seed + t as u32)));
        for _ in 0..GAME_TICKS + 10 {
            let pads = [0, 1].map(|t| cpus[t].as_mut().map_or(0, |c| c.pad(&g)));
            g.step(pads);
        }
        assert_eq!(g.phase, Phase::Final);
        g.score
    }

    #[test]
    fn level_indices_round_trip() {
        assert_eq!(Level::from_index(0), Some(Level::Novice));
        assert_eq!(Level::from_index(1), Some(Level::Pro));
        assert_eq!(Level::from_index(2), Some(Level::AllStar));
        assert_eq!(Level::from_index(3), None);
    }

    #[test]
    fn both_cpus_score_in_a_full_game() {
        for seed in 1..4 {
            let score = play([Some(Level::Pro), Some(Level::Pro)], seed);
            assert!(score[0] > 0 && score[1] > 0, "seed {seed}: {score:?}");
        }
    }

    #[test]
    fn every_level_beats_a_player_who_stands_still() {
        for level in [Level::Novice, Level::Pro, Level::AllStar] {
            let score = play([None, Some(level)], 3);
            assert!(score[1] > score[0] + 10, "{level:?}: {score:?}");
        }
    }

    #[test]
    fn harder_levels_win_more() {
        for (strong, weak) in [(Level::AllStar, Level::Pro), (Level::Pro, Level::Novice)] {
            let (mut s, mut w) = (0, 0);
            for seed in 1..4 {
                let a = play([Some(strong), Some(weak)], seed);
                let b = play([Some(weak), Some(strong)], seed);
                s += a[0] + b[1];
                w += a[1] + b[0];
            }
            assert!(s > w, "{strong:?} {s} vs {weak:?} {w}");
        }
    }

    #[test]
    fn buttons_are_tapped_not_held() {
        let mut g = Game::new(5);
        let mut cpu = Cpu::new(1, Level::AllStar, 9);
        let mut prev = 0;
        for _ in 0..GAME_TICKS / 4 {
            let pad = cpu.pad(&g);
            assert_eq!(pad & prev & (PASS | SHOOT | SWITCH), 0);
            prev = pad;
            g.step([0, pad]);
        }
    }
}
