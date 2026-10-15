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

### Presentation polish
Hand-authored sprite sheets and more poses (dunks, defence), cheerleaders and photographers,
crowd reactions, an exhibition/attract mode, a playoff mode with save codes, sound and music.
