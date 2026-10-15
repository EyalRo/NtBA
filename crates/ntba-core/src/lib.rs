//! NtBA game core.
//!
//! Everything in this crate is platform independent: the simulation is a pure
//! function of (state, input) advanced at a fixed [`TICK_HZ`], and the renderer
//! draws into an in-memory 320x200 paletted framebuffer. Front ends (the
//! WebAssembly build today, a native window later) only feed input in and copy
//! pixels out.

pub mod font;
pub mod game;
pub mod input;
pub mod palette;
pub mod people;
pub mod render;
pub mod replay;
pub mod sprite;

/// Virtual framebuffer size (pixels). Scaled up with nearest-neighbour by the host.
pub const SCREEN_W: usize = 320;
pub const SCREEN_H: usize = 200;

/// Fixed simulation rate.
pub const TICK_HZ: u32 = 60;
pub const DT: f32 = 1.0 / TICK_HZ as f32;

/// Court dimensions in feet. +x runs baseline to baseline, +y runs sideline to sideline.
pub const COURT_L: f32 = 94.0;
pub const COURT_W: f32 = 50.0;
