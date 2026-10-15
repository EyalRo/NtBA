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
- Input is a `u8` bitmask per pad (`ntba_core::input`); `ntba_step` packs home in the low byte
  and away in the next. That keeps replays, tests and a future
  lock-step P2P protocol (exchange one byte per tick) trivial.
- The core renders into a 320x200 paletted framebuffer (see *Period constraints* below) and
  expands it to RGBA. The page scales the canvas with CSS `image-rendering: pixelated`.
- The wasm module uses a C ABI, not `wasm-bindgen`, so the only build requirement is the
  `wasm32-unknown-unknown` target. Keep the exported surface tiny (`ntba-wasm/src/lib.rs`).

## Period constraints

We deliberately work within 1990 PC limits, which is what the reference games used:

- **320x200**, like VGA mode 13h (the Amiga/DOS screenshots we studied are all 320x200). The page
  displays it at **4:3**, as CRTs did, so pixels are slightly taller than wide.
- **Palette of at most 256 colours** (`palette::MAX_COLOURS`), 6 bits per channel (`palette::vga`).
  We use fewer than 100, authored as **4-tone ramps** per material (dark, shadow, base, light) so
  shapes are shaded by hand-picking tones rather than by blending.
- No true colour, alpha or blending: shadows darken by remapping palette indices, soft edges and
  lighting use **ordered dithering** (`render::bayer`), as period games did.
- A test (`palette::tests`, `render::tests`) enforces the palette limit and that frames stay
  inside it.

## Sprites

`sprite.rs` is a tiny paletted-canvas toolkit: shaded ellipses and capsules lit from the top-left,
text, and a **selective outline** (every edge takes the darkest tone of the colour it borders).
`people.rs` builds the figures from it: a skeleton-driven run cycle with forward kinematics for the
legs and two-bone IK so the dribbling arm follows the ball, front/back versions for the end-on
camera, and seated bench figures. Spectators in the crowd use the same toolkit at smaller scales.
**Everything is procedural (generated in code) today, not hand-drawn sprite art.** Replacing it
with hand-authored sprite sheets in the same palette is a drop-in change inside those two
modules; the simulation and scene code only ask for "a person with this pose".

## Camera views

The simulation owns a `View` (`Side`, or `End` of either basket) because the view changes what the
direction keys mean. The ball position drives it with hysteresis (`END_VIEW_ENTER_X`,
`END_VIEW_EXIT_X` in `game.rs`, mirrored for the left basket): near either basket the game cuts to
the **end-on** camera (looking at that basket from midcourt, `render::project_end`), and back to the **side** camera in transition. In the end-on view
the direction keys are screen-relative: Up runs toward the basket, Left/Right strafe. A direction
held across a cut keeps its old meaning until released, so the cut never reverses a run mid-stride.
The renderer picks sprites (`people::side_person` / `people::end_person`) and projections from
`Game::view`.

## Rendering notes

- The camera scrolls horizontally to follow the ball (`Framebuffer::update_camera`, visual only,
  eased per rendered frame, clamped so one end of the court is always in view). The crowd scrolls
  slower than the floor for parallax.
- `render::project(cam_x, x, y, z)` maps feet to pixels. Scale grows linearly from `FAR_SCALE` to
  `NEAR_SCALE` px/ft between the far and near sidelines, which gives the gently slanted baselines of
  the originals; the floor is drawn by inverting that mapping per pixel (`draw_court`).
- Hardwood planks, lines and lane colours are computed from world coordinates, so they stay put
  while the camera moves; lighting and dithering are applied per screen pixel.
- The player you control wears black shoes, as in the Genesis manual.

## Simulation rules of thumb

- Fixed timestep, no wall-clock, no threads. Randomness comes from the seeded xorshift in `Game`.
- Stick to `+ - * /` and `sqrt` on `f32` inside `game.rs`; avoid `sin`/`cos`/`powf` there. These
  are the operations that stay bit-identical across machines, which matters for P2P lock-step.
  Visual-only code in `render.rs` is free of this restriction.
- Court units are feet: x is baseline-to-baseline (0..94), y sideline-to-sideline (0..50),
  z is height. `render::project` maps these to screen pixels.

## Current gameplay (proof of concept)

Two humans share the keyboard, five-on-five. `Game::players` holds ten players: 0..5 are home
(gold, attack the right-hand basket, WASD + `Z`/`X`/`C`), 5..10 are away (green, attack the left,
arrows + `J`/`K`/`L`). `Game::step` takes one input mask per team; each human steers
`controlled[team]`, the other four are AI: on offence they run to half-court spots (mirrored for
away), on defence each guards its counterpart (same slot on the other team) from the basket side.
Shoot scores 2 or 3 by distance (chance falls with distance); pass goes to the teammate best
aligned with the held direction (or the nearest). On defence, shoot is a jump that may steal a held
ball within reach or block a shot just released; a defender standing in a passing lane intercepts
it. Switch moves control to the teammate nearest the ball whenever your team doesn't have it.
Misses bounce off the rim and anyone can rebound. A basket or a shot-clock violation gives the ball
to the other team. Shot clock 24 s, one 5-minute period, Enter restarts after the buzzer.

## Where the next milestones plug in

- **Bot opponent / P2P:** a `controller` per team (human, bot, remote) producing the same `u8`
  input mask that `Game::step` already takes per team — that is also the seam for P2P.
- **Sprites:** player/ball/referee drawing in `render.rs` is procedural today; swap for
  hand-authored sprite sheets (indexed pixels in the same 32-colour palette) without touching
  the simulation.
- **Audio:** the host will play sounds from event flags the core exposes (not implemented yet).
