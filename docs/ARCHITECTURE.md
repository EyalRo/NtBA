# Architecture

```
crates/ntba-core   platform-independent game: simulation + software renderer (no deps)
crates/ntba-wasm   cdylib exposing ntba-core through a plain C ABI
web/               static host page: canvas, keyboard, fixed-timestep loop
scripts/           build-web.sh, serve.sh
context/           reference material about the 1989 original (links and notes)
docs/              vision, architecture
```

## Data flow

```
keyboard ─▶ web/main.js ─ buttons bitmask ─▶ ntba_step()  ─▶ Game::step      (60 Hz fixed tick)
canvas  ◀─ RGBA bytes ◀── ntba_framebuffer() ◀ ntba_render() ◀ render::draw   (once per frame)
```

- The host owns time. It accumulates real elapsed time and calls `ntba_step` once per 1/60 s,
  then renders once per animation frame. The core never reads a clock.
- Input is a `u8` bitmask (`ntba_core::input`). That keeps replays, tests and a future
  lock-step P2P protocol (exchange one byte per tick) trivial.
- The core renders into a 320x200 paletted framebuffer (32 colours, `render::PALETTE`; the Genesis
  could show 64, DOS 16) and expands it to RGBA. The page scales the canvas with CSS `image-rendering: pixelated`.
- The wasm module uses a C ABI, not `wasm-bindgen`, so the only build requirement is the
  `wasm32-unknown-unknown` target. Keep the exported surface tiny (`ntba-wasm/src/lib.rs`).

## Rendering notes

- The camera scrolls horizontally to follow the ball (`Framebuffer::update_camera`, visual only,
  eased per rendered frame, clamped so one end of the court is always in view). The crowd scrolls
  slower than the floor for parallax.
- `render::project(cam_x, x, y, z)` maps feet to pixels. Scale grows linearly from `FAR_SCALE` to
  `NEAR_SCALE` px/ft between the far and near sidelines, which gives the slanted baselines of the
  originals; the floor is drawn by inverting that mapping per pixel (`draw_court`).
- Hardwood planks, lines and lane colours are computed from world coordinates, so they stay put
  while the camera moves. Sprites (`draw_person`, `draw_seated`) are drawn procedurally with
  rectangles; replace them with hand-authored sheets when art is ready.
- The player you control wears black shoes, as in the Genesis manual.

## Simulation rules of thumb

- Fixed timestep, no wall-clock, no threads. Randomness comes from the seeded xorshift in `Game`.
- Stick to `+ - * /` and `sqrt` on `f32` inside `game.rs`; avoid `sin`/`cos`/`powf` there. These
  are the operations that stay bit-identical across machines, which matters for P2P lock-step.
  Visual-only code in `render.rs` is free of this restriction.
- Court units are feet: x is baseline-to-baseline (0..94), y sideline-to-sideline (0..50),
  z is height. `render::project` maps these to screen pixels.

## Current gameplay (proof of concept)

One team of five attacks the right-hand basket with no defence. Arrow keys / WASD move the
controlled player; `X` shoots (2 or 3 points by distance; chance falls with distance); `Z` passes
to the teammate best aligned with the held direction (or the nearest if none is held); `C` /
Space switches player when the ball is loose. Misses bounce off the rim and a teammate recovers
them. Shot clock 24 s, one 5-minute period, Enter restarts after the buzzer.

## Where the next milestones plug in

- **Opposition / bot AI:** a second `[Player; 5]` in `Game` and a `controller` per player
  (human, bot, remote) producing the same `u8` input mask — that is also the seam for P2P.
- **Sprites:** player/ball/referee drawing in `render.rs` is procedural today; swap for
  hand-authored sprite sheets (indexed pixels in the same 32-colour palette) without touching
  the simulation.
- **Audio:** the host will play sounds from event flags the core exposes (not implemented yet).
