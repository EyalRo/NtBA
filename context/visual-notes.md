# Visual notes from the reference screenshots

Observations from the three images in [screenshots/](screenshots/) and how they compare with the
NtBA proof of concept (320x200, 16 colours, whole court on screen). Written by us; add to it.

## What the originals do

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

## Target look: a slightly later EA game

[screenshots/later-ea-game-target-look.png](screenshots/later-ea-game-target-look.png) (provided
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

## How NtBA compares today

The proof of concept was reworked to chase this look (see `docs/screenshot.png`). Status:

| Aspect | Original | NtBA now | Still to do |
| --- | --- | --- | --- |
| Camera | Scrolling half-court | Scrolls to follow the ball, eased | Lead the camera on fast breaks |
| Court perspective | Slanted baselines (1990) to mild (later) | Mild perspective projection | Tune the amount of skew |
| Hardwood | Multi-tone planks with breaks | Four close tones, random lengths, butt joints, centre logo | Grain, wear |
| Lane colour | Yellow / blue | Blue lane and centre circle | Centre-circle logo |
| Hoop | Backboard, arm, stanchion, net, shot clock | All of these; shot clock on the stanchion | Backboard shatter/vibration, net swish |
| Players | Numbered jerseys, varied looks | 34 px sprites, numbers, 3 skin tones + blond, run cycle with bob, black shoes = controlled | Hand-authored sprites, more poses (dunk, defend) |
| Scoreboard | Amber dot-matrix, framed (compact in the later game) | Compact framed panel, dot-matrix digits | Team names, period |
| Bench & table | Bench with coaches, scorer's table, ad boards | Both benches, standing coach, scorer's table, officials, ad boards | Cheerleaders, photographers, animation |
| Crowd | Tiers, waving arms | Parallax tiers with aisles and waving fans | More variety and reactions to scores |
| Shot mechanic | Hold and release at the apex, tap = fake | Instant shot | See genesis-manual-notes.md |

None of this is required for the current milestone (single-player versus a bot); it is a
backlog of presentation details that carry the "televised game" feel the project is after.
