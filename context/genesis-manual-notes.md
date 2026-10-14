# Notes from the Sega Genesis manual (1990 release)

Our own summary of *Lakers vs. Celtics and the NBA Playoffs* (Genesis, US manual, 40 pages).
The manual is hosted at Sega Retro:
<https://segaretro.org/images/1/1d/Lakers_Vs_Celtics_MD_US_Manual.pdf>. It is copyrighted, so the
PDF is not stored in this repository; page numbers below refer to the printed page numbers in it.
The Genesis version differs from the 1989 DOS original in places (it uses 1990 playoff rosters and
a 3-button pad), so treat this as a close relative, not the exact reference.

## Controls (pp. 2–3, 17–21)

A 3-button pad plus START. You always control the player wearing **black shoes** (player 2 gets
gray shoes in a two-player game). On offence that is always the ball handler.

| Button | Offence | Defence |
| --- | --- | --- |
| D-pad | Move/dribble; diagonals allowed | Move the controlled defender |
| A | Shoot: quick tap = **fake**, slow press and release = shot (release at the top of the jump) | Jump to **block** |
| B | **Pass** (to the player closest to and facing the passer, or to whoever the D-pad points at) | **Switch** to the defender nearest the ball; repeated presses cycle the others |
| C | **Time out** (must have possession) | **Steal** attempt ("rip") |
| START | Pause (and open the stats/substitution screen) | Pause |

- The receiver of a pass gets the black shoes, i.e. control follows the ball. (NtBA does this.)
- Press A *before* dribbling to fake out a defender who is right in your face.
- Once the ball handler stops dribbling he cannot dribble again (see travelling below).
- A shot's success depends on (1) the player's statistics, (2) how closely he is guarded,
  (3) the timing of the A-button release. The game picks the shot type (hook, jumper, slam,
  finger roll…) from the player's position and distance.
- Defenders can be double-teamed or sent to box out; the controlled defender only follows the
  action "with his eyes" unless the player moves him.
- Steals that reach in too aggressively draw fouls.

## Rules the game enforces (pp. 18–19, 28–30)

- **Free throws:** 2 when fouled while a defender reaches in; 2 when fouled shooting and
  missing; 1 when fouled shooting and scoring.
- **Turnovers/violations:** travelling (including double-dribble); 24-second shot clock (the shot
  clock is shown above each backboard; it resets on a shot); charging (offensive foul);
  10-second backcourt; backcourt violation (cannot cross back over halfcourt once advanced);
  5-second inbound; out of bounds.
- **Time outs:** 5 per half, unused ones do not carry over, plus a free time out after each period.
- **Fouling out (Simulation mode):** 6 personal fouls, offensive fouls included.
- **Fatigue (Simulation mode):** a 4-to-1 rating drops as players tire; tired players are slower,
  jump lower and shoot worse. Slam dunks tire players the most.

## Game options (pp. 10–13, 22–27)

- Game type: single game or playoff tournament (tournament forces Simulation, Showtime,
  1 player, 12-minute periods; best-of-5 early rounds, best-of-7 final, 6-character passwords to resume).
- Players: 1, 2, or demo (computer plays both sides).
- Play mode: **Arcade** (no fatigue, fouls don't count) vs **Simulation**.
- Play level: Pre-Season (forgiving refs, weaker opponents), Regular, Showtime (tight refs, physical).
- Period length: 2, 5, 8 or 12 minutes; four periods; 5-minute overtime (2 minutes if playing
  2-minute periods).
- Music on/off — with music off you hear the ball, the whistle and the crowd.

## Presentation worth borrowing (pp. 8, 15–16, 24)

- Title screen with scrolling credits, then a short self-playing exhibition game.
- Pre-game "EA Sports Network" broadcast: starting-lineup introductions with season stats.
- Starting tip-off: press A to send the big man up for the jump ball.
- Stats screen between periods (substitutions, fouls); halftime report on other games.
- Foul/violation calls shown in a text box with a portrait of the referee ("CHARGING").
- Home team wears mostly white; no built-in home-court advantage.
- Scoreboard shows the shot clock above each backboard.

## How NtBA's proof of concept compares

| Original | NtBA POC today | Candidate follow-up |
| --- | --- | --- |
| A = shoot with release timing, tap = fake | `X` shoots instantly | Hold-and-release timing, tap to fake |
| B = pass, C = steal/time out | `Z` = pass, `C` = switch player | Remap to the original's three-button model (pass / shoot / action) |
| Travelling, 10 s backcourt, 5 s inbound, out of bounds | Not modelled | Add once there is a defence and inbounding |
| Shot quality from stats + defender closeness | Distance only | Needs opposition and player attributes |
| Free throws, fouls | None | After defence exists |
| Periods 2/5/8/12 min, OT | One 5-minute period | Configurable periods, overtime |
| Shot clock above each basket | Shown on the scoreboard | Optional |

Rosters, team names and player data in the manual are deliberately **not** reproduced here:
NtBA ships with generic fictional teams.
