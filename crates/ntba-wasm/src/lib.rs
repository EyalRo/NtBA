//! WebAssembly front door for `ntba-core` and `ntba-net`.
//!
//! A plain C ABI is used on purpose: no `wasm-bindgen`, no `wasm-pack`, no JS glue
//! generation. The host (see `web/main.js`) calls `ntba_init`, then every 1/60 s
//! `ntba_step(buttons)`, and once per animation frame `ntba_render()` followed by
//! a copy of `RGBA_LEN` bytes starting at `ntba_framebuffer()` into a canvas.

use std::cell::RefCell;

mod net;

use ntba_core::cpu::{Cpu, Level};
use ntba_core::game::{Game, Phase};
use ntba_core::render::{self, Framebuffer};
use ntba_core::replay::{InputLog, DETERMINISM_SEED, DETERMINISM_SEED_HASH, DETERMINISM_TICKS};
use ntba_core::{SCREEN_H, SCREEN_W, TICK_HZ};

struct State {
    game: Game,
    fb: Framebuffer,
    /// An online match in progress (or being set up); see `net.rs`.
    online: Option<net::Online>,
    /// The computer opponent of a VS CPU game.
    cpu: Option<Cpu>,
}

impl State {
    /// The game being shown: the online match once it has started, else the solo game.
    fn shown(&self) -> &Game {
        self.online
            .as_ref()
            .and_then(|o| o.session.game())
            .unwrap_or(&self.game)
    }
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.borrow_mut().as_mut().map(f))
}

/// Create (or restart) a game. Must be called before anything else.
#[no_mangle]
pub extern "C" fn ntba_init(seed: u32) {
    STATE.with(|s| {
        let mut fb = Framebuffer::new();
        // A restart keeps the player's display setting.
        fb.colourblind = s.borrow().as_ref().is_some_and(|old| old.fb.colourblind);
        *s.borrow_mut() = Some(State {
            game: Game::new(seed),
            fb,
            online: None,
            cpu: None,
        })
    });
}

/// Hand `team` (0 = home, 1 = away) to the computer at `level` (0 Novice, 1 Pro, 2 All-Star).
#[no_mangle]
pub extern "C" fn ntba_cpu(team: u32, level: u32, seed: u32) {
    with_state(|s| {
        s.cpu = match Level::from_index(level) {
            Some(level) if team < 2 => Some(Cpu::new(team as usize, level, seed)),
            _ => None,
        }
    });
}

/// Advance one fixed tick; `buttons` holds the home pad's byte, then the away pad's (CPU's is ignored).
#[no_mangle]
pub extern "C" fn ntba_step(buttons: u32) {
    with_state(|s| {
        let mut pads = [buttons as u8, (buttons >> 8) as u8];
        if let Some(cpu) = &mut s.cpu {
            pads[cpu.team()] = cpu.pad(&s.game);
        }
        s.game.step(pads)
    });
}

/// Draw the current game state into the framebuffer.
#[no_mangle]
pub extern "C" fn ntba_render() {
    with_state(|s| {
        let State {
            game, fb, online, ..
        } = s;
        let game = online
            .as_ref()
            .and_then(|o| o.session.game())
            .unwrap_or(game);
        render::draw(game, fb)
    });
}

/// Turn colour-blind-safe team colours on (1) or off (0): the away team wears blue, not green.
#[no_mangle]
pub extern "C" fn ntba_colourblind(on: u32) {
    with_state(|s| s.fb.colourblind = on != 0);
}

/// Pointer into wasm linear memory at the RGBA framebuffer (`width * height * 4` bytes).
/// Re-read it after every `ntba_render`; do not cache it across memory growth.
#[no_mangle]
pub extern "C" fn ntba_framebuffer() -> *const u8 {
    with_state(|s| s.fb.rgba.as_ptr()).unwrap_or(std::ptr::null())
}

#[no_mangle]
pub extern "C" fn ntba_width() -> u32 {
    SCREEN_W as u32
}

#[no_mangle]
pub extern "C" fn ntba_height() -> u32 {
    SCREEN_H as u32
}

#[no_mangle]
pub extern "C" fn ntba_tick_hz() -> u32 {
    TICK_HZ
}

/// Points scored by `team` (0 = home, 1 = away).
#[no_mangle]
pub extern "C" fn ntba_score(team: u32) -> u32 {
    with_state(|s| s.shown().score.get(team as usize).copied().unwrap_or(0)).unwrap_or(0)
}

/// How many times sound cue `id` (see `ntba_core::game::Cue`) has fired in the game being shown.
#[no_mangle]
pub extern "C" fn ntba_cue(id: u32) -> u32 {
    with_state(|s| s.shown().cues.get(id as usize).copied().unwrap_or(0)).unwrap_or(0)
}

/// Ticks played by the game being shown (low 32 bits); a drop means a new game.
#[no_mangle]
pub extern "C" fn ntba_ticks() -> u32 {
    with_state(|s| s.shown().ticks as u32).unwrap_or(0)
}

/// 1 once the game being shown has reached the final buzzer, else 0. The host uses it to leave the
/// match and return to the title screen.
#[no_mangle]
pub extern "C" fn ntba_game_over() -> u32 {
    with_state(|s| u32::from(s.shown().phase == Phase::Final)).unwrap_or(0)
}

/// Replays the fixed synthetic input script and returns the low 32 bits of the final state
/// hash. Together with `ntba_determinism_hash_hi` and the expected value it lets any build
/// prove it simulates exactly like the reference (see `scripts/check-determinism.mjs`).
#[no_mangle]
pub extern "C" fn ntba_determinism_hash_lo() -> u32 {
    determinism_hash() as u32
}

#[no_mangle]
pub extern "C" fn ntba_determinism_hash_hi() -> u32 {
    (determinism_hash() >> 32) as u32
}

#[no_mangle]
pub extern "C" fn ntba_determinism_expected_lo() -> u32 {
    DETERMINISM_SEED_HASH as u32
}

#[no_mangle]
pub extern "C" fn ntba_determinism_expected_hi() -> u32 {
    (DETERMINISM_SEED_HASH >> 32) as u32
}

fn determinism_hash() -> u64 {
    InputLog::synthetic(DETERMINISM_SEED, DETERMINISM_TICKS)
        .replay()
        .state_hash()
}
