# Roadmap and wishlist

## Milestones

| Stage | Scope | Status |
| --- | --- | --- |
| **POC** | One team, no opposition. Run, pass, shoot, score. Shot clock, game clock. Playable in the browser. Period-style look with a scrolling side view and an end-on half-court view. | Done |
| **MVP** | Single-player versus a bot: five-on-five with defence, steals/blocks, hold-and-release shooting with fakes, fouls and free throws, the main violations, periods and overtime, sound. | Next |
| **Online** | Peer-to-peer play between two humans, built on the deterministic input-per-tick core. | Planned |
| **Desktop** | Native build of the same Rust core. | Planned |

## Wishlist

Ideas we like and have not committed to. Unordered. Nothing here blocks the POC or the MVP.

### Selectable team size (1-5 players per side)
Let the player choose how many players each team fields, from 1-on-1 up to the full 5-on-5. The
simulation, bot and camera should take the on-court count as a parameter rather than assuming five
(1-on-1 is the *One-on-One* lineage, 3-on-3 is a streetball-style half-court game). Affects
spacing, defensive assignments, the lineup UI and the roster/stats HUD.

### 1984 mode (period-accurate graphics)
A selectable **1984 look** alongside the default early-90s one: a low-resolution, 16-colour
home-computer style with chunky sprites, a flat court on a black background, a simple scoreboard
(names, score, clock, shot clock) and big blocky text — the feel of early-80s one-on-one games such
as EA's *Dr. J and Larry Bird Go One-on-One* (1983) on the Commodore 64. It is a renderer/palette
swap over the same core (a second palette and sprite set at a lower resolution), pairs naturally
with the 1-on-1 team-size option above, and, like everything else here, uses **original** art
only — no assets copied from those games.

### Create-a-player
Let people **create their own players**, as *TV Sports: Basketball* (Cinemaware, 1990) did. Name,
jersey number, position, height/weight, skin and hair, and a budget of attribute points (shooting,
speed, jump, passing, defence, stamina) that feed straight into the simulation. Custom players are
stored locally and can be shared in online play. This also fits the project's "no real rosters"
rule: creators supply the identities.

### TV presentation (far future — not POC, not MVP)
The late-80s/early-90s games felt like a televised broadcast. A long-term goal is to bring that
back with **original** content:

- A **presenter** who opens the game, introduces the starting lineups and reports at halftime
  (*TV Sports: Basketball* opens with a desk presenter; the 1989 EA game has an in-studio
  halftime show).
- **Commercials**: fictional, tongue-in-cheek ad breaks shown between periods and at halftime
  (the Cinemaware game inserts mock commercials).

All presenters, brands and jokes must be our own and fictional — no content copied from the games
that inspired this. Packaging them as swappable content packs (so the community can add their own)
is worth considering.

### More camera work
- A dedicated free-throw view and a tip-off view (the end-on camera already exists for half-court play).
- Replays of baskets.
- Smarter camera lead on fast breaks.

### Stats HUD and minimap
A scoreboard panel with the two rosters, points, personal fouls and time outs, plus a small
minimap of the court showing where everyone is — as in *TV Sports: Basketball*'s side view.

### Gameplay depth (from the Genesis manual notes)
Hold-and-release shot timing and fakes, steals/blocks, the full set of violations, time outs and
substitutions, arcade vs simulation modes. See `context/genesis-manual-notes.md`.

### Hand-authored sprites (replace the procedural art)
Players, crowd, hoop and furniture are **procedural today**: code draws shaded shapes into the
palette (`sprite.rs`, `people.rs`, `render.rs`). It reads as pixel art but is a stand-in. The goal
is **hand-drawn sprite sheets** (original art, same 256-colour VGA-style palette and 320x200
canvas) with proper animation frames — run, dribble, shoot, dunk, defend, celebrate — loaded in
place of the generators. The sprite-building functions are the only code that needs to change.
Contributions from pixel artists are welcome; see `CONTRIBUTING.md`.

### Presentation polish
More poses (dunks, defence), cheerleaders and photographers,
crowd reactions, an exhibition/attract mode, a playoff mode with save codes, sound and music.
