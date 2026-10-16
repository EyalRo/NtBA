# How to play

**Play now: https://eyalro.github.io/NtBA/** — nothing to install.

Remember when sports games came in a big cardboard box, the court was made of chunky pixels, and
the scoreboard glowed amber? **NtBA** brings that feeling back: lace up, dribble down the floor
under the arena lights, and sink one while the pixel crowd goes wild.

![NtBA side view](https://raw.githubusercontent.com/wiki/EyalRo/NtBA/images/game-side-view.png)
![NtBA end-on half-court view](https://raw.githubusercontent.com/wiki/EyalRo/NtBA/images/game-end-view.png)

## What you can do today

- 🏀 **Two players, one keyboard**: five-on-five, each of you steers one player and the AI runs the
  other four, against the shot clock and the game clock
- 🌐 **Two players, two computers**: meet in the online lobby, chat and challenge someone, or send
  a friend a room code (see [Online Play](Online-Play))
- 🛡️ **Real defence**: run into the ball handler to strip the ball, swipe for steals, block shots,
  jump passing lanes
- 🎥 **Two camera angles**: a sideline view for the break, and an end-on view when you attack the hoop
- 🎯 **Smart shooting**: long shots are riskier, threes are worth the gamble
- 🏟️ **Arena atmosphere**: a packed crowd, benches, referees and a glowing scoreboard
- 🖥️ **Plays right in your browser**

> **Early days.** This is a playable prototype. A computer opponent is next, so for now bring a
> friend, locally or online.

## Controls

| Home (gold) | Away (green) | Action |
| --- | --- | --- |
| **WASD** | Arrow keys | Run (on the end-on view, **Up** runs toward the basket) |
| **X** | **K** | Shoot; on defence, swipe for a steal or jump to block |
| **Z** | **J** | Pass; hold a direction to pick your target |
| **C** | **L** | Switch to the teammate nearest the ball |
| **Enter** | | Play again after the final buzzer |

On defence, run into the ball handler to steal the ball. Lost your player off screen? An arrow in
your shirt colour on the edge of the screen points the way. Click the game if the keys don't
respond.

## Title menu

The page opens on a title screen over an idle demo with two buttons: **Local game** (a fresh local
game with two pads; the **Main menu** button takes you back) and **Online**. Online is hidden when
online play is not configured or your browser has no WebRTC.

## Playing online

**Online** opens the lobby.

1. Pick a name (1 to 20 characters; letters, digits, Hebrew and emoji are fine). It is remembered
   in your browser. Others see you as `name#ABC`, where the suffix comes from a random id that
   changes every time you connect.
2. The lobby lists everyone online (up to 16), whether they are in the lobby or in a match, and
   whether you are connected to them. Other players in the lobby can see your IP address; that is
   how peer-to-peer works.
3. **Chat** with everyone in the lobby. Hebrew and English both work, and each line aligns to its
   own language. Messages are up to 500 characters, nothing is saved, and each player has a **Mute**
   button.
4. Press **Challenge** next to a connected player who is in the lobby. They have 30 seconds to
   **Accept** or **Decline**. A player who is in a match cannot be challenged.
5. The challenger plays Home (gold) and the challenged player plays Away (green). Both of you can use
   either key set (WASD or the arrow keys) for your own player.

**With a room code instead:** one of you presses **Host an online game** and gets a 6-character code
(valid for 10 minutes); the other types it into **Join with code** and presses **Join**. The host
plays Home and the guest plays Away.

If the page says it couldn't connect directly, some networks block peer-to-peer connections: try
again or try another network. How it works: [Online Play](Online-Play).

## Where we're headed

Beating a computer-controlled team. Playing a friend online. 1-on-1 street ball. Players with real
skills (shooters, passers, thieves) and positions like forward and center. A **1984 mode** with
chunky early-80s graphics and chip-tune sound. Broadcast-style presentation. See the
[Roadmap](Roadmap); everything planned and every idea lives in the
[issue tracker](https://github.com/EyalRo/NtBA/issues).

## Fine print

NtBA is an independent fan project, not affiliated with or endorsed by any league, team or player.
All art and sound are original. Released under the
[GPL-3.0-or-later](https://github.com/EyalRo/NtBA/blob/main/LICENSE) license.
