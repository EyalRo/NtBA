# NtBA — Vision and handoff

This is the original project brief the repository was started from, kept verbatim in spirit so
contributors share the same goals. See [ARCHITECTURE.md](ARCHITECTURE.md) for how the code is
organised today and the root [README](../README.md) for how to run it.

## What this is

NtBA is an open-source basketball game that recreates the look and feel of the late-1980s era of
the sport on home computers — specifically the genre anchored by *Lakers versus Celtics and the
NBA Playoffs* (Electronic Arts, 1989).

It is a **five-on-five** basketball game built around a fixed, elevated sideline view of the full
court, rendered in **faithful 1980s pixel art**: a visible, limited palette, a detailed hardwood
court with painted markings, tiny individually-animated players, a crowded arena, benches,
officials, and a period-style broadcast scoreboard.

The goal is to reproduce the *experience* of that era, not to apply a pixel-art filter on top of a
modern game: recognizable action, readable player motion, and arena atmosphere.

The name is a deliberate pun — **NtBA: Not the Basketball Association** — an independent project
with no affiliation to any real league. Team names, player identities and rosters are out of scope
for now. What matters first is the game itself: court, animation, flow of play, scoring, clock.

## Why this exists

The 1989 original was an early *licensed* NBA game that made specific, real stars part of the
gameplay and presented the game like a broadcast, with starting-lineup introductions and a
halftime show. NtBA asks whether that experience can be built faithfully, with modern tooling, as
an open project — a love letter to a specific era of sports gaming, and an exercise in building a
playable basketball simulation from first principles, pixel art included.

Historical footnote: the title celebrates the Celtics–Lakers rivalry, but the Detroit Pistons won
the 1989 NBA championship. Nostalgia rarely matches the box score.

## Core principles

- **Faithful, not filtered.** Authentic period pixel art and presentation: low resolution,
  nearest-neighbour scaling, a deliberately limited palette, hand-authored sprites and arena.
- **The game comes first.** Names, rosters and real-league identity are deferred. A complete,
  playable five-on-five loop is the priority; generic fictional teams stand in.
- **Own everything.** Original sprites and sounds inspired by the era — never copyrighted game
  assets.
- **Open by design.** Public, open-source, GPL.

## Technical direction

- **Rust compiled to WebAssembly.** Simulation, rules, ball physics, player AI and scoring live in
  Rust; the browser handles display, input and audio. The same core can later run natively.
- **Online peer-to-peer play.** Two players face each other directly, no central game server.
  Browser first, architecture kept compatible with a desktop build.
- **Bot play.** With no human opponent, an AI opponent plays a believable game. The same AI is
  the building block for filling a court with active players on both sides.
- **Single-player versus AI is the first milestone:** a recognizable retro court, ten animated
  players, a controllable basketball, a scoring loop, and an opponent that plays back.

## Open questions / deferred decisions

- Player identities and rosters — deliberately out of scope initially.
- Exact internal resolution and palette — tune against reference research (see `context/`).
- Online transport details — P2P is the direction; the concrete protocol is for implementation.
- Desktop packaging — planned, but secondary to a stable browser game.

## Addendum: more inspiration

Beyond the 1989 EA game, two other games shape the target:

- ***TV Sports: Basketball*** (Cinemaware, 1990) is the graphics-fidelity reference. Its camera
  switches between a sideline view for transition play and an end-on view looking at the basket
  for half-court play, and it presents the game as a TV broadcast with a desk presenter and mock
  commercials. It also lets players create their own players. Those last three ideas are on the
  [wishlist](ROADMAP.md) (presentation is far future).
- A slightly later EA basketball game is the reference for the floor, sprite and HUD look.

See `context/` for notes and sources. As always, everything we ship is original.
