# Handoff: hand-authored sprite sheets for players and other assets

Audience: whoever (human artist, or an agent driving a pixel editor) produces the art, plus the
engineer who wires it into the renderer. Everything below is derived from the code at
`75deda4`; file and symbol names are exact.

## 1. Goal and non-goals

**Goal.** Replace the procedurally generated figures with hand-authored, indexed-colour sprite
sheets in the *existing* 89-colour palette, so the game reaches the "TV Sports: Basketball / Bulls
vs. Blazers" look described in [Visual Notes](Visual-Notes) (pose variety, readable faces and numbers,
dunks, defence stances).

**Non-goals.**
- No change to simulation (`game.rs`), netcode or the state hash. Rendering is visual only.
- No true colour, alpha blending or new palette entries unless unavoidable (see §6).
- No change to the 320x200 framebuffer or the camera/projection maths.

[Architecture](Architecture) already states the intended seam: *"Replacing it with hand-authored sprite
sheets in the same palette is a drop-in change inside `sprite.rs`/`people.rs`; the simulation and
scene code only ask for 'a person with this pose'."*

## 2. How sprites work today (read before drawing anything)

| Item | Where | Notes |
| --- | --- | --- |
| Canvas | `sprite.rs::Sprite { w, h, ox, oy, pix: Vec<u8> }` | Paletted `u8` pixels; `CLEAR = 255` is transparent. `(ox, oy)` is the draw origin, **feet centre** for people. `blit(sx, sy, plot)` places origin at screen `(sx, sy)`. |
| Palette | `palette.rs::PALETTE` (89 entries, indices 0..=88) | 4-tone **ramps** `[dark, shadow, base, light]` per material, contiguous from index 2. Light comes from the **top-left**. |
| Outline | `Sprite::outline()` | 1 px, each edge takes `dark_of()` the colour it borders (the ramp's darkest tone), black for non-ramp pixels. Art must leave a 1 px transparent margin so the outline fits, or bake it in. |
| Side person | `people.rs::side_person(look, f, pose, black_shoes)` | Canvas 60x64, origin (30, 56). Figure about 34 px tall; `Cam::body_h()` says 47 px incl. headroom for the controlled-player marker. `f` = +1 facing right, -1 left (art is drawn once, mirrored for the other side). |
| End person | `people.rs::end_person(look, back, pose, black_shoes)` | Canvas 48x56, origin (24, 46). About 27 px tall. `back` = seen from behind (running toward the basket), else front. |
| Seated | `people.rs::seated_person(jacket, skin, hair, suit)` | Canvas 36x40, origin (18, 36). About 24 px tall. Bench players, coaches, officials at the scorer's table. |
| Spectator | `render.rs::spectator(h, scale, dim, cheer)` | Canvas 18x20, origin (9, 14); tiny, scaled per crowd row. |
| Ball | `render.rs::draw_ball` | Canvas 12x12, origin (6, 6); 7 px ball. |
| Shadows | `Framebuffer::shadow` | Drawn separately in code; **do not paint shadows into sprites**. |
| Draw call | `Framebuffer::sprite(&Sprite, x, y)` | Nothing else is needed from a replacement: build a `Sprite` and return it. |

### Pose inputs the renderer already supplies (`people.rs::Pose`)
`phase` (run-cycle radians, `p.anim * 0.55`), `moving` (speed² > 4), `jump` (jump-shot airborne),
`ball_hand: Option<(dx, dy)>` (ball position relative to the feet, already mirrored for facing).
These are the *only* animation states available today, so the first sheet only needs to cover:

| State | Side view | End view (front and back) |
| --- | --- | --- |
| Idle | 1 frame | 1 frame |
| Run | **8 frames** (phase 0..2π; current code is continuous, 8 is the usual hand-drawn cycle) | 4 frames |
| Jump / shoot | 1-2 frames (rise, apex) | 1 frame |
| Dribble | Run and idle variants with the ball hand reaching down to the ball | 1 frame (hand beside body) |

Currently the dribble arm tracks the ball with IK. With fixed frames the ball must instead be
drawn relative to a per-frame **hand anchor**; see §5.

### Variation axes that must survive (`people.rs::Look`)
- `skin`: 3 ramps (`SKIN_LIGHT`, `SKIN_MID`, `SKIN_DARK`)
- `hair`: 3 ramps (`HAIR_BLACK`, `HAIR_BROWN`, `HAIR_BLOND`)
- `top` / `trim` / `shorts`: kit ramps (home: `GOLD` / `PURPLE` / `PURPLE`; away: `GREEN` / `CLOTH` / `GREEN`, or colour-blind `BLUE` / `CLOTH` / `NAVY`)
- `number`: 0-99 drawn with the 3x5 game font (`font.rs`) in `trim[2]`
- `stripes` (referee: black bars on white) and `suit` (coach/officials: long trousers, shirt and tie)
- `black_shoes`: shoes are `SUIT` instead of `CLOTH` for the player you control

Rosters (`render.rs::ROSTERS`) are 2 teams x 5 players of (skin, hair, number). That is 3x3 = 9
body variants times kits, which is why sheets must be authored in **palette-role colours**, not
final colours (§3).

## 3. Authoring convention: role-indexed "template" sprites

Do **not** paint a separate sheet per team/skin/hair. Paint one template per pose and recolour at
load time. Reserve these template ramps (all existing palette ranges, so no palette change):

| Role | Template ramp used while drawing | Remapped at runtime to |
| --- | --- | --- |
| Skin | `SKIN_MID` (idx 6-9) | `SKINS[look.skin]` |
| Hair | `HAIR_BROWN` (34-37) | `HAIRS[look.hair]` |
| Jersey top | `GOLD` (14-17) | `look.top` |
| Trim/number | `PURPLE` (18-21) | `look.trim` |
| Shorts | `PINK` (58-61) | `look.shorts` |
| Socks, shoes | `CLOTH` (22-25) | stays `CLOTH`; shoes become `SUIT` for `black_shoes` via a second role `STEEL` (26-29) -> `shoe_ramp()` |
| Eyes, mouth, ball lacing, panel lines | `BLACK` (0) | unchanged |
| Ball (when baked into hands) | `BALL` (42-45) | unchanged |

Remap rule: for a template pixel with index `i` in a role ramp, output `target_ramp[i - ramp_start]`.
Tone order (0 dark .. 3 light) is preserved so shading survives. The outline pass is run **after**
remapping so outlines take the final colour's dark tone.

Why these template ramps: they are visually distinct from each other in any editor, so an artist
can tell skin from shorts at a glance. Document the template palette as a Photoshop/Aseprite
`.gpl`/`.pal` generated from `palette::PALETTE` (task §7.1).

## 4. Deliverable asset list

Priority order. Sizes are the current ones; keep canvases and origins (feet centre) identical so
the render code does not change.

### P0: players (side view), template-coloured
- Facing right only (mirror in code for left). Canvas 60x64, origin (30, 56), figure ~34 px, 1 px clear margin.
- Idle, Run x8, Jump/shoot x2, Dribble-idle, Dribble-run x8 (hand low at ball height).
- Strongly consider **ball-less and ball-holding** variants. The ball itself keeps being drawn by `draw_ball` at `ball_screen()`.

### P0: players (end view), front and back
- Canvas 48x56, origin (24, 46), figure ~27 px.
- For each of front and back: Idle, Run x4, Jump x1, Dribble x1.
- Back view shows the number big and legible (this is the view you spend most time in).

### P1: new poses unlocking gameplay polish (need new `Pose` inputs, see §5.3)
- Defence stance (arms out, knees bent), side and end.
- Block jump (arms up), steal swipe, pass (chest pass and release), shot release, celebration, dejected, fall/stumble on a strip.
- Dunk (1-2 frames; needs a new trigger first).

### P1: non-player people
- **Referee** side (idle, run, whistle) and end; striped template (`stripes`) or a fixed painted version.
- **Coach** side + end (standing, arms folded, pointing). Suit template (`suit`).
- **Seated** bench players (home/away jacket recolour via `jacket` role), coaches, table officials.
- **Cheerleaders and photographers** (listed as "not yet" in [Visual Notes](Visual-Notes)).

### P2: crowd and set dressing
- Spectator heads+shoulders at 3 scales, normal and arms-up, with 6 shirt recolours. Currently generated per call from a hash.
- Scorer's-table props, benches, towel/water cooler, courtside ad-board lettering (already text-drawn).
- Hoop and net animation frames (swish, rim vibrate, backboard shatter), hoop back/front in both views.
- **Ball**: 7 px ball, 3-4 spin frames, plus an end-view ball with squash on the bounce.
- **HUD/title**: team logos (home and away crest, ~24x24, 4-tone), title logotype, menu cursor ball. (Box art is the JPEG at `web/box-art.jpg`; the logo should stay consistent with it.)
- **Court decals**: centre-circle "NTBA" logo is drawn procedurally; a painted bitmap would replace it.

## 5. Integration design

### 5.1 Sheet format (recommended)
- **Source of truth**: one 8-bit **indexed PNG per sheet** under `assets/sprites/` (e.g. `players_side.png`, `players_end.png`, `staff.png`, `crowd.png`, `props.png`), using the game's palette order *exactly* (PNG palette entry N == `PALETTE[N]`, entry 255 = transparent). Plus a sibling `*.json` manifest:
  ```json
  { "frame": [60, 64], "origin": [30, 56],
    "clips": { "idle": {"start": 0, "len": 1},
               "run":  {"start": 1, "len": 8},
               "jump": {"start": 9, "len": 2} },
    "anchors": { "hand_near": [[x, y], ...per frame...], "hand_far": [...] } }
  ```
- Why PNG+JSON: editable in Aseprite/Pixelorama/GIMP, diffable via a conversion step, and artists never touch Rust.
- **Build step**: `scripts/pack-sprites.mjs` (Node, matching `make-sfx.mjs` style; add `make sprites`) converts PNG+JSON into a compact binary (`crates/ntba-core/src/sprites.bin`, one byte per pixel, RLE optional) that `ntba-core` embeds with `include_bytes!`. This keeps the core `no_std`-friendly, dependency-free (`[dependencies]` is empty today) and deterministic, and avoids shipping a PNG decoder inside the wasm.
- Check generated binaries in (like `web/audio/*.wav`, "don't edit by hand; change the source and regenerate") so `cargo build` needs no Node.

### 5.2 Code changes (engineer)
1. New module `crates/ntba-core/src/sheet.rs`: `struct Sheet { frame, origin, clips, anchors, data }`, `fn frame(&self, clip, i) -> Sprite` (copy pixels into a `Sprite`), `fn recolour(&mut Sprite, &Look)` implementing §3, with unit tests for role remapping.
2. In `people.rs`, keep the procedural functions as the **fallback/legacy** and add `side_person_sheet` / `end_person_sheet` selected from `render.rs::draw_player`. Select run frame with `((pose.phase / TAU) * len) as usize % len`.
3. Replace IK dribble: draw frame, then position the ball at `hand_near[frame]`. `ball_screen()` currently computes the ball position independently, so for sheets either (a) use the anchor to override it in side view, or (b) keep the ball where it is and use a "hand reaches low" frame (simpler; recommended for v1).
4. Mirror for `facing = -1` with a flip on blit (`Sprite::flipped()`); the art is lit from top-left, so a mirrored sprite flips its light direction. Two options: accept it (the original games did), or draw a separate left-facing sheet for the main characters. **Decision for the artist: accept mirroring in v1.**
5. Seated/spectator/ball: same pattern, lower priority.
6. Tests to keep green: `cargo test --workspace` (palette-limit and "frames stay inside the palette" tests in `render::tests`), `make determinism` (render is outside the hash, but run it anyway), plus a new test that every sheet pixel is `CLEAR` or `< 89`.

### 5.3 New poses need small additions on the Rust side
`Pose` only knows moving/jump/ball_hand. Defence, block, steal, pass, celebrate would need new
booleans (or an enum `Action`) populated in `draw_player` from `Game` state. Read, never write
`game.rs` state for this; if the info is not already exposed (e.g. a stance or "just stole" timer),
**flag it before changing any `pub` field**, since `Game` is the networked, hashed state.

## 6. Palette guidance (hard constraints)

- Max 256 entries (`MAX_COLOURS`); we use 89. Free indices are 89..=255 (255 is `CLEAR`, so effectively 89..=254).
- Prefer re-using existing ramps. If an asset truly needs a new material (e.g. a team logo orange), add a **4-tone ramp** in `palette.rs` after `LED_DIM`, **append only**: existing indices must not shift. `dark_of()` relies on ramps living between index 2 and `RAMPS_END = 78`, so a new ramp *past* `LED_DIM` will not get automatic outlines unless `dark_of`/`RAMPS_END` is extended. Raise this before adding one.
- 6-bit VGA channels (`vga()`): colours are quantised to multiples of 4 on 8 bits. Pick values already on that grid.
- Colour-blind mode swaps away `GREEN` for `BLUE`/`NAVY` via the `look.top/shorts` roles; keep the sheet in role colours and this works for free. Do not paint green or blue directly on a jersey.

## 7. Task breakdown

1. **Palette export** (~1 h): script/`make` target dumping `PALETTE` to `assets/sprites/ntba.gpl` and a 1-row indexed PNG, so any editor loads the exact palette.
2. **Reference render dump** (~1-2 h): a small test or example that renders each current procedural pose to PNG templates at native size (`side_person` with `STILL`, 8 run phases, jump, dribble; `end_person` front/back; `seated_person`). Artists use these as underlays/traces; it also fixes the exact pixel footprints (feet position, head height, width). Put outputs under `assets/sprites/reference/`.
3. **Sheet format + packer + loader** (`sheet.rs`, `scripts/pack-sprites.mjs`, §5.1-5.2) with tests (~1 day).
4. **P0 art**: side players, then end players. Review each against `assets/screenshot*.png` in-engine.
5. **Wire up and A/B**: behind a feature flag or a build constant `USE_SHEETS`, compare against procedural frames side by side in `make serve`.
6. **P1 poses + `Pose` extension**, then P1/P2 props, crowd, logos.
7. **Docs**: update [Architecture](Architecture) (Sprites section currently says everything is procedural) and [Visual Notes](Visual-Notes) ("Hand-authored sheets, more poses" row), and add `assets/sprites/README.md` documenting the role palette, naming and how to run `make sprites`.

## 8. Acceptance criteria

- Players at least as readable as today at 320x200: number legible on the back view, three clearly different skin tones and three hair colours, home vs away distinct (including colour-blind mode, key **B**).
- Run cycle has no foot sliding at the speeds the sim produces and loops seamlessly.
- All pixels in the shipped binary are palette indices `< 89` or `CLEAR`.
- Controlled player still identifiable (black shoes via the `STEEL` template role) and the marker triangle still fits above the head (`Cam::body_h()` = 47 px side / 38 px end).
- `make check` and `make determinism` pass; wasm size growth documented (budget suggestion: < 150 KB raw for all sheets).
- Procedural fallback remains until sheets cover every `Look` combination the game can request.

## 9. Open questions for the project owner

1. Hand-drawn left-facing art vs. mirrored right-facing (§5.2 item 4)? Default: mirror.
2. Is an external artist / generation pipeline producing the PNGs, or is the goal an agent-authored pixel-by-pixel pipeline? This changes whether §7.2 (reference dumps) is the main source.
3. Who owns the licence/provenance of the art? The wiki states "we take ideas, not art" from the reference games; keep it that way (no traced screenshots).
4. Is a larger sprite size (e.g. 40+ px per [Visual Notes](Visual-Notes) "players are drawn about twice as tall") wanted now? It changes `Cam::body_h`, canvases and court density, and is a separate change; this handoff assumes **no size change**.

## 10. Build notes

- Build/test: `make check`, `make web`, `make serve` (port 8080), `make determinism`. Rust toolchain pinned in `rust-toolchain.toml`.
- Track the tasks in section 7 as GitHub issues under the `milestone: polish` label (see [Roadmap](Roadmap)).
- Nothing in this page has been implemented yet; it is analysis plus plan only.
