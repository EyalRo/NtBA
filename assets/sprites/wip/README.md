# Work-in-progress player sprite boards

Target-look presentation boards (1536x1024, true colour) showing the proposed player art. **Not** packable
sheets: they are not indexed, not on a 60x64 / 48x56 grid and contain labels. See
[Sprite Sheets Handoff](https://github.com/EyalRo/NtBA/wiki/Sprite-Sheets-Handoff) for the target format.

- `players-board-v1.png`: side (idle 4, run 6, jump 4, dribble 4, per team) and end view (idle 2, run 4, jump 4, dribble 4) plus end-view roster variants.
- `players-board-v2.png`: revision with in-game reference shots, adds pass, shoot and land poses and close-ups.

## Demo animation

`python3 scripts/render_wip_board_demo.py` cuts the v2 board's run, dribble and shoot frames out of the image, aligns them on a shared ground line and renders `assets/sprites/previews/wip_board_v2_demo.gif`: walk left, walk up (back view), dribble right, jump shot with the ball flying to the hoop. Requires Pillow, numpy and scipy.

`reference/` holds crops of the v2 board (2x, nearest neighbour) and the frames extracted for the demo, for embedding in the art brief (#140).
