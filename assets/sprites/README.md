# NtBA sprite sources

Initial original template-coloured player sheets, at the existing native frame sizes and feet-centre origins. Indexed PNGs contain the exact 89 game palette entries in order; palette index 255 is transparent.

- `players_side.png`: 20 frames, 60x64 each: idle, run x8, jump x2, dribble idle, dribble-run x8.
- `players_end.png`: 14 frames, 48x56 each: front idle/run x4/jump/dribble, then back idle/run x4/jump/dribble.
- JSON manifests define clips, origins, and template role ramps.
- `ntba.gpl` exports the Rust palette table for pixel-art editors.

Template roles: skin 6-9; hair 34-37; jersey 14-17; trim 18-21; shorts 58-61; socks 22-25; shoes 26-29. Recolour at runtime. These are source assets only; loader/remapping integration and in-engine validation remain future work, so procedural sprites should remain the fallback. Artwork is original and not traced from reference-game imagery.
