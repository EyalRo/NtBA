//! Input recording and replay.
//!
//! The simulation is a pure function of the seed and the per-tick input bytes, so a log of
//! those is a complete recording of a match: it drives replays, regression tests and the
//! cross-build determinism check, and it is the same data online play exchanges.

use crate::game::Game;
use crate::input::{DOWN, LEFT, PASS, RIGHT, SHOOT, SWITCH, UP};

/// Parameters and expected result of the cross-build determinism check: replaying
/// `InputLog::synthetic(DETERMINISM_SEED, DETERMINISM_TICKS)` must end on exactly this hash on
/// every platform (native, wasm, every browser). Update it only when simulation behaviour is
/// changed on purpose, and bump the game version used for online play at the same time.
pub const DETERMINISM_SEED: u32 = 1;
pub const DETERMINISM_TICKS: usize = 3_600;
pub const DETERMINISM_SEED_HASH: u64 = 0x471144f3bc0e556c;

/// A recorded match: the seed plus the `[home, away]` pads for every tick, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputLog {
    pub seed: u32,
    pub pads: Vec<[u8; 2]>,
}

impl InputLog {
    pub fn new(seed: u32) -> Self {
        InputLog {
            seed,
            pads: Vec::new(),
        }
    }

    /// A deterministic, busy input script (movement, passes, shots, switches) for tests and
    /// for checking that different builds of the core agree.
    pub fn synthetic(seed: u32, ticks: usize) -> Self {
        let mut log = InputLog::new(seed);
        let mut x = seed | 1;
        for _ in 0..ticks {
            let mut pads = [0u8; 2];
            for pad in &mut pads {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                let dirs = [UP, DOWN, LEFT, RIGHT, UP | RIGHT, DOWN | LEFT];
                *pad = dirs[(x % 6) as usize];
                match (x >> 8) % 40 {
                    0 => *pad |= PASS,
                    1 => *pad |= SHOOT,
                    2 => *pad |= SWITCH,
                    _ => {}
                }
            }
            log.push(pads);
        }
        log
    }

    pub fn push(&mut self, pads: [u8; 2]) {
        self.pads.push(pads);
    }

    pub fn len(&self) -> usize {
        self.pads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pads.is_empty()
    }

    /// Replay the whole log from a fresh game and return the final state.
    pub fn replay(&self) -> Game {
        let mut g = Game::new(self.seed);
        for &pads in &self.pads {
            g.step(pads);
        }
        g
    }

    /// State hash after every tick (`len()` entries); used to find the first diverging tick.
    pub fn hashes(&self) -> Vec<u64> {
        let mut g = Game::new(self.seed);
        self.pads
            .iter()
            .map(|&pads| {
                g.step(pads);
                g.state_hash()
            })
            .collect()
    }

    /// Compact encoding: seed (u32 LE), tick count (u32 LE), then two bytes per tick.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.pads.len() * 2);
        out.extend_from_slice(&self.seed.to_le_bytes());
        out.extend_from_slice(&(self.pads.len() as u32).to_le_bytes());
        for p in &self.pads {
            out.extend_from_slice(p);
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let seed = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?);
        let n = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize;
        let body = bytes.get(8..)?;
        if body.len() != n.checked_mul(2)? {
            return None;
        }
        Some(InputLog {
            seed,
            pads: body.chunks_exact(2).map(|c| [c[0], c[1]]).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_reproduces_the_same_hash() {
        let log = InputLog::synthetic(7, 3_000);
        assert_eq!(log.replay().state_hash(), log.replay().state_hash());
        assert_eq!(log.hashes(), log.hashes());
    }

    #[test]
    fn replay_matches_live_play() {
        let log = InputLog::synthetic(99, 1_500);
        let mut live = Game::new(log.seed);
        for &pads in &log.pads {
            live.step(pads);
        }
        assert_eq!(live.state_hash(), log.replay().state_hash());
    }

    #[test]
    fn different_inputs_or_seeds_change_the_hash() {
        let a = InputLog::synthetic(7, 600);
        let mut b = a.clone();
        // Compare at the tick of the change: later ticks may legitimately converge again
        // (a player pinned against the boundary, a possession reset).
        b.pads[300][0] ^= UP;
        assert_eq!(a.hashes()[..300], b.hashes()[..300]);
        assert_ne!(a.hashes()[300], b.hashes()[300]);
        assert_ne!(
            InputLog::synthetic(7, 600).replay().state_hash(),
            InputLog::synthetic(8, 600).replay().state_hash()
        );
    }

    #[test]
    fn hash_changes_every_tick_of_a_running_game() {
        let hashes = InputLog::synthetic(3, 120).hashes();
        assert!(hashes.windows(2).all(|w| w[0] != w[1]));
    }

    #[test]
    fn bytes_round_trip_and_reject_garbage() {
        let log = InputLog::synthetic(5, 50);
        assert_eq!(InputLog::from_bytes(&log.to_bytes()), Some(log.clone()));
        let mut bytes = log.to_bytes();
        bytes.pop();
        assert_eq!(InputLog::from_bytes(&bytes), None);
        assert_eq!(InputLog::from_bytes(&[1, 2, 3]), None);
    }

    /// Golden value: if this changes, simulation behaviour changed. That is fine when
    /// intended but it must
    /// never differ between native and wasm builds; CI also checks the wasm build.
    #[test]
    fn golden_hash_is_stable() {
        let h = InputLog::synthetic(DETERMINISM_SEED, DETERMINISM_TICKS)
            .replay()
            .state_hash();
        assert_eq!(h, DETERMINISM_SEED_HASH, "state hash changed: {h:#018x}");
    }
}
