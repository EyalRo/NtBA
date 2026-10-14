# NtBA — Not the Basketball Association

An open-source, five-on-five basketball game in faithful late-1980s pixel art, inspired by
*Lakers versus Celtics and the NBA Playoffs* (Electronic Arts, 1989). Rust compiled to
WebAssembly, played in the browser. Independent project — no affiliation with any real league; no
real teams or players.

**Status: proof of concept.** One team, no defence: run, pass, shoot, score against a shot clock
and a 5-minute game clock. See [docs/VISION.md](docs/VISION.md) for where this is headed
(opposing bot team, online peer-to-peer play, a native build).

![NtBA proof of concept](docs/screenshot.png)

## Play it

```sh
nix develop        # or install Rust via rustup; rust-toolchain.toml pins everything
make serve         # builds the wasm and serves http://localhost:8080
```

Open http://localhost:8080 and click the page if keys don't respond.

| Key | Action |
| --- | --- |
| Arrows / WASD | Move the highlighted player |
| `X` / `K` | Shoot (3 pts from beyond the arc; farther shots miss more often) |
| `Z` / `J` | Pass to the teammate in the direction you hold (nearest if none) |
| `C` / Space | Switch player while the ball is loose |
| Enter | Restart after the final buzzer |

Without Nix: `rustup show && make serve` (needs Python 3 and `make`).

## Repository layout

| Path | Contents |
| --- | --- |
| `crates/ntba-core` | Simulation, rules and software pixel renderer (platform independent) |
| `crates/ntba-wasm` | WebAssembly build exposing the core through a small C ABI |
| `web/` | Static page: canvas, keyboard, fixed 60 Hz tick loop |
| `docs/` | [Vision](docs/VISION.md) and [architecture](docs/ARCHITECTURE.md) |
| `context/` | Links and notes about the 1989 original |
| `flake.nix`, `rust-toolchain.toml` | Reproducible dev shell / pinned toolchain |

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md); `make check` must pass. Every asset must be original —
nothing copied from commercial games.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
