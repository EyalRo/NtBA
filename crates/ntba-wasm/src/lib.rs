//! WebAssembly front door for `ntba-core`.
//!
//! A plain C ABI is used on purpose: no `wasm-bindgen`, no `wasm-pack`, no JS glue
//! generation. The host (see `web/main.js`) calls `ntba_init`, then every 1/60 s
//! `ntba_step(buttons)`, and once per animation frame `ntba_render()` followed by
//! a copy of `RGBA_LEN` bytes starting at `ntba_framebuffer()` into a canvas.

use std::cell::RefCell;

use ntba_core::game::Game;
use ntba_core::render::{self, Framebuffer};
use ntba_core::{SCREEN_H, SCREEN_W, TICK_HZ};

struct State {
    game: Game,
    fb: Framebuffer,
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
        *s.borrow_mut() = Some(State {
            game: Game::new(seed),
            fb: Framebuffer::new(),
        })
    });
}

/// Advance one fixed tick; the low byte of `buttons` is the home pad, the next byte the away pad.
#[no_mangle]
pub extern "C" fn ntba_step(buttons: u32) {
    with_state(|s| s.game.step([buttons as u8, (buttons >> 8) as u8]));
}

/// Draw the current game state into the framebuffer.
#[no_mangle]
pub extern "C" fn ntba_render() {
    with_state(|s| render::draw(&s.game, &mut s.fb));
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
    with_state(|s| s.game.score.get(team as usize).copied().unwrap_or(0)).unwrap_or(0)
}
