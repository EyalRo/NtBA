# Contributing to NtBA

Thanks for helping! NtBA is GPL-3.0-or-later; by contributing you agree your work is released
under that license. Everything in the repo must be original or compatibly licensed — **no assets
copied from commercial games**.

## Setup

Either:

- **Nix:** `nix develop` (or `direnv allow`) gives you the pinned Rust toolchain with the wasm
  target, `wasm-opt`, Python and `make`. Flakes must be enabled. The first run creates
  `flake.lock`; commit it if it changes.
- **Rustup:** `rustup show` inside the repo installs what `rust-toolchain.toml` asks for. You
  also need Python 3 and `make`.

## Everyday commands

| Command | What it does |
| --- | --- |
| `make serve` | Build the wasm and serve http://localhost:8080 |
| `make web` | Build `web/ntba.wasm` only |
| `make test` | `cargo test --workspace` |
| `make check` | Everything CI runs: fmt, clippy (`-D warnings`), tests, wasm build |
| `make fmt` | Format the code |

## Ground rules

- Keep `make check` green; CI runs the same thing.
- Game logic goes in `crates/ntba-core` with unit tests there, not in the wasm crate or JS.
- Keep `game.rs` deterministic (see `docs/ARCHITECTURE.md`): no clocks, no float transcendentals,
  randomness only through the seeded RNG.
- Keep the wasm export surface small and change it deliberately; the JS in `web/` depends on it.
- Small, focused pull requests; describe what you saw in the browser for visual changes (a
  screenshot of *our* rendering is welcome).
- Reference research and notes about the original game go in `context/` (links and notes, not
  copyrighted screenshots).
