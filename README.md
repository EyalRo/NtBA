<h1 align="center">🏀 NtBA</h1>
<p align="center"><b>Not the Basketball Association</b></p>

<p align="center">
  <img src="web/box-art.jpg" alt="NtBA box art" width="340">
</p>

<p align="center">
  <i>Remember when sports games came in a big cardboard box, the court was made of chunky pixels,<br>
  and the scoreboard glowed amber? Yeah. We missed that too.</i>
</p>

<p align="center">
  <a href="https://eyalro.github.io/NtBA/"><img src="https://img.shields.io/badge/▶_PLAY_NOW-in_your_browser-f5a623?style=for-the-badge" alt="Play now"></a>
  &nbsp;
  <a href="https://github.com/EyalRo/NtBA/wiki"><img src="https://img.shields.io/badge/📖_WIKI-the_serious_stuff-4a90d9?style=for-the-badge" alt="Wiki"></a>
</p>

---

## 🕹️ Play it right now

### 👉 **[eyalro.github.io/NtBA](https://eyalro.github.io/NtBA/)** 👈

No install. No account. No 40 GB download. Open the page, grab the keyboard, tip off.

NtBA is an open-source, five-on-five basketball game in the spirit of the late-80s home-computer
classics: a sideline camera, tiny hand-animated pixel players, a packed arena, referees, benches
and a glowing broadcast scoreboard. Everything (art and sound) is original.

<p align="center">
  <img src="assets/screenshot.png" alt="NtBA gameplay, sideline view" width="48%">
  <img src="assets/screenshot-end-view.png" alt="NtBA gameplay, end-on half-court view" width="48%">
</p>
<p align="center"><sub>Sideline view for the fast break, end-on view when you attack the hoop.</sub></p>

## 🌐 Play a friend online

Two browsers, zero game servers.

1. Hit **Host an online game** and get a 6-character code.
2. Send the code to your friend.
3. They type it into **Join with code**. Tip-off.

Under the hood it's peer-to-peer: WebRTC data channels plus deterministic lock-step netcode, so the
two browsers only swap a few bytes of input per frame and both simulate the exact same game, down
to the last bit. A tiny Cloudflare Worker just introduces the two of you, then gets out of the way.
Want the nerdy details? They're in the wiki: [Online Play](https://github.com/EyalRo/NtBA/wiki/Online-Play).

## 🎮 Controls

| Home (gold) | Away (green) | Action |
| --- | --- | --- |
| **WASD** | Arrow keys | Run |
| **X** | **K** | Shoot (on defence: swipe for a steal or jump to block) |
| **Z** | **J** | Pass (hold a direction to pick your target) |
| **C** | **L** | Switch to the teammate nearest the ball |
| **Enter** | | Play again after the final buzzer |

Run into the ball handler to strip the ball. Keys not responding? Click the game first.

## ✨ What's in the box

- 🏀 Five-on-five, one keyboard or two computers
- 🛡️ Real defence: strips, steals, blocks, jumped passing lanes
- 🎥 Two camera angles that switch with the action
- 🎯 Risk/reward shooting: long shots are riskier, threes are worth the gamble
- 🏟️ Crowd, benches, refs and a glowing scoreboard

> **Early days.** It's a playable prototype. A computer opponent is next, so for now bring a friend.

## 🛠️ Build it yourself

Rust compiled to WebAssembly, with a plain JS front end (no framework, no bundler).

```sh
make serve   # builds web/ntba.wasm and serves http://localhost:8080
make check   # fmt, clippy, tests, wasm build: everything CI runs
```

You'll need a Rust toolchain (`rustup` picks up the pinned one and the `wasm32` target
automatically) and Python 3 for the tiny dev server. There's also a `flake.nix` if you're a Nix
person.

## 🗺️ Where to go from here

Everything that isn't "go play it" lives elsewhere, on purpose:

- 📖 **[Wiki](https://github.com/EyalRo/NtBA/wiki)**: how to play, vision, architecture, roadmap
- 🐛 **[Issues](https://github.com/EyalRo/NtBA/issues)**: bugs, ideas, and everything planned

---

<p align="center"><sub>
NtBA is an independent fan project, not affiliated with or endorsed by any league, team or player.<br>
Released under the <a href="LICENSE">GPL-3.0-or-later</a>. Go sink one. 🏀
</sub></p>
