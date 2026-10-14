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

## How NtBA compares today

| Aspect | Original | NtBA POC | Idea |
| --- | --- | --- | --- |
| Camera | Scrolling half-court | Whole court, fixed | Scrolling camera + bigger sprites (e.g. 32x48) |
| Court perspective | Oblique end, lane skewed | Symmetric trapezoid | Try the oblique projection once the camera scrolls |
| Hardwood | Multi-tone planks with breaks | Two alternating tones | Add a third tone and pseudo-random plank breaks |
| Lane colour | Yellow / blue | Red | Pick a distinct team colour (fictional) |
| Hoop | Backboard, arm, stanchion, net, shot clock | Edge-on backboard and small net | Draw a 3/4 hoop with a shot-clock box |
| Players | Numbered jerseys, varied looks | Identical sprites | Numbers and 3–4 skin/hair variants |
| Scoreboard | Amber dot-matrix, framed | Amber blocky digits | Dot-matrix digit font |
| Bench & table | Bench with coaches, scorer's table | Small bench strip | Scorer's table, coach figures |

None of this is required for the current milestone (single-player versus a bot); it is a
backlog of presentation details that carry the "televised game" feel the project is after.
