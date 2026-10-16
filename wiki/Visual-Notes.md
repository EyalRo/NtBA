# Visual notes from the reference screenshots

Observations from the three images in [Reference Screenshots](Reference-Screenshots) and how they compare with the
NtBA proof of concept (320x200, 16 colours, whole court on screen). Written by us; add to it.

## What the originals do

![DOS version](https://raw.githubusercontent.com/wiki/EyalRo/NtBA/images/dos-ega-free-throw-lineup.webp)
![Genesis gameplay](https://raw.githubusercontent.com/wiki/EyalRo/NtBA/images/genesis-gameplay-half-court.webp)

**Camera**
- Neither version shows the whole court. The view **scrolls horizontally** to follow the ball and
  shows roughly half the court at a time, so sprites can be large (about 1/6 to 1/9 of the screen
  height) and readable. (Tip-off screen: centre court is centred, both benches visible.)
- It is a raised sideline view with an **oblique end**: the baseline and the lane are skewed on a
  diagonal toward the basket rather than being drawn square-on (Genesis gameplay shot, DOS shot).
  Sidelines stay horizontal.

**Court**
- DOS: flat orange-brown court, thin light lines, a yellow lane/free-throw area and yellow centre
  circle, grey out-of-bounds apron, red surround. Only EGA colours.
- Genesis: hardwood drawn as **horizontal planks in several alternating tan/orange shades with
  random breaks**; thick black lines; a blue painted lane and a blue centre circle with a logo;
  free-throw circle and three-point arc clearly drawn; short tick marks on the sidelines.
- The lane is a different colour from the floor in both versions (yellow DOS, blue Genesis).

**Hoop**
- Drawn as a real object, not a marker: backboard seen in 3/4 perspective with a steel arm and
  padded stanchion, rim and white net, a **shot-clock readout mounted on the stanchion** (Genesis).
  The ball visibly drops through the net.

**Arena**
- Crowd in tiered stands behind the far sideline, with individually drawn spectators and a few
  waving arms; red/dark seating bands on DOS, a dark crowd on Genesis.
- Both benches are drawn at the far sideline with seated players in warm-up jackets, coaches in
  suits, folding chairs; the **scorer's table** (blue skirted table, officials, computers, phone)
  sits at half court; a referee stands near the table; photographers and cheerleaders sit beside
  the baseline hoop.

**Players**
- Jersey numbers are visible on the shirt (both versions), team colours are distinct (green vs
  gold/purple) with white shoes and socks, and each side has varied skin and hair (a blond player
  among others). Running animation has clear leg and arm swing; poses are specific (hands raised
  on defence, ball held low, tip-off crouch).
- Players are drawn about twice as tall relative to the screen as ours.

**HUD**
- Amber dot-matrix (LED) digits on a black panel with a white frame: team names, period ("PER."),
  clock and scores. DOS puts it in the top-left corner and overlays team logos; Genesis centres it
  at the top, framed like a stadium scoreboard.
- A text prompt ("PRESS START TO BEGIN") and team scores in the bottom strip on the pre-game
  screen. Shot clock shown at the baskets, not only on the scoreboard.

## *TV Sports: Basketball* (Cinemaware, 1990): camera views and presentation

Five screenshots supplied by the project owner (not stored in this repository). The owner's
verdict: this is the graphics fidelity we are after.

- **End-on half-court view.** Looking at the basket from midcourt: backboard and rim at the top
  centre, stands with red steps and crowd in the top corners, referee and coaches behind the
  baseline, a light orange court with pale lines and a cream lane. Players are seen from behind and
  from the front. A free-throw situation uses the same view, with players lined up along the lane.
- **Side view for the backcourt.** A horizontal view of the centre circle with the pack of players
  running, used when the ball is out of the frontcourt. Above it is a **stats panel**: both team
  names, time outs, a roster list with jersey numbers, names, points and personal fouls, and a
  small **court minimap** with the clock in the middle.
- **TV presentation.** A mock **commercial** interstitial (a framed beer ad with a slogan) and a
  **presenter** at a desk with a microphone in front of the arena, with subtitles — the game plays
  like a broadcast.

What NtBA did with it: the simulation now switches between a side view and an end-on view
(`Game::view`), with screen-relative controls on the end-on view (see
[Architecture](Architecture)). The end-on view has its own front/back
sprites, a face-on backboard with the shot clock above it, staff behind the baseline and a court
that converges toward the baseline. Not done yet: the stats panel and minimap, a dedicated
free-throw view, and — far future — the presenter and commercials ([Roadmap](Roadmap)).

## Target look: *Bulls versus Blazers and the NBA Playoffs* (EA)

![Later EA game, target look](https://raw.githubusercontent.com/wiki/EyalRo/NtBA/images/later-ea-game-target-look.png)

[later-ea-game-target-look.png](images/later-ea-game-target-look.png) (provided
by the project owner as "where we would like to get to"). What stands out, and what we did about it:

- **Subtle floor.** Low-contrast planks in a light tan, thin 1-pixel black lines, a painted logo in
  the centre circle, dashed free-throw circle inside the lane. NtBA now uses closer plank tones,
  one-pixel lines and an "NTBA" centre logo.
- **Mild perspective.** Sidelines horizontal, only a gentle slant at the lane. We reduced the
  far/near scale ratio from 1.8 to 1.4.
- **Big, dynamic players.** About 1/6 of the screen height, wide running strides with the body
  bobbing, different skin tones and hair, two clearly distinct kits (white/grey vs red). NtBA
  sprites are now 34 px tall (17% of the screen) with a four-frame run cycle and body bob.
- **Tight framing.** The bench, a standing coach in a suit and the far wall take only the top
  quarter of the screen so the court dominates. NtBA now gives the court 57% of the height.
- **Courtside ad boards** on the wall (fictional ones here) and a **compact scoreboard** in the
  top-left corner instead of a big centred panel.
- Not copied yet: the second kit (no opposition yet), distinct shadows under players, the lane
  painted in team colour with a dashed ring (we have the ring, in blue).

## Resolution and colours of the references

All the screenshots we studied are **320x200** (the TV Sports and DOS captures are exactly that
size; the others are 2x/4x upscales of it). That was VGA's mode 13h with **256 colours** on DOS,
32 colours (+ extra-half-brite) on the Amiga, and 16 on EGA; the Genesis showed 64 of 512 at 320x224.
These games were shown on 4:3 monitors, so pixels were slightly taller than wide. NtBA's choice:
320x200, a 256-colour VGA-style palette (we use under 100), 6-bit channels, 4:3 display. We do
*not* use 640x480: it was an option on PC (16 colours) but the genre did not use it and it would
change the look.

## How NtBA compares today

![NtBA side view](https://raw.githubusercontent.com/wiki/EyalRo/NtBA/images/game-side-view.png)

The proof of concept was reworked to chase this look (see the screenshots on [How to Play](How-to-Play)). Status:

| Aspect | Original | NtBA now | Still to do |
| --- | --- | --- | --- |
| Camera | Scrolling half-court | Scrolls to follow the ball, eased | Lead the camera on fast breaks |
| Court perspective | Slanted baselines (1990) to mild (later) | Mild perspective projection | Tune the amount of skew |
| Hardwood | Multi-tone planks with breaks | Four close tones, random lengths, butt joints, centre logo | Grain, wear |
| Lane colour | Yellow / blue | Blue lane and centre circle | Centre-circle logo |
| Hoop | Backboard, arm, stanchion, net, shot clock | All of these; shot clock on the stanchion | Backboard shatter/vibration, net swish |
| Players | Numbered jerseys, varied looks, shaded and outlined | 34 px shaded outlined sprites, skeleton run cycle, IK dribble arm, numbers, 3 skin tones and hair colours, black shoes = controlled | Hand-authored sheets, more poses (dunk, defend) |
| Scoreboard | Amber dot-matrix, framed (compact in Bulls vs. Blazers) | Compact framed panel, dot-matrix digits | Team names, period |
| Bench & table | Bench with coaches, scorer's table, ad boards | Both benches, standing coach, scorer's table, officials, ad boards | Cheerleaders, photographers, animation |
| Crowd | Tiers, waving arms | Five parallax rows of shaded fans with aisles, waving arms and camera flashes | Signs and banners, reactions to scores |
| Shot mechanic | Hold and release at the apex, tap = fake | Instant shot | See [Genesis Manual Notes](Genesis-Manual-Notes) |

**Development Version badge.** A small "DEVELOPMENT VERSION" plate sits flush with the top-right
corner of the 320x200 framebuffer: amber lamp lettering in the game's own font on a black plate with
a bevelled steel frame, like the other overlay plates. It is drawn last so nothing covers it, and it
is outside the simulation, so it never affects game state or hashes.

None of this is required for the current milestone (single-player versus a bot); it is a
backlog of presentation details that carry the "televised game" feel the project is after.
