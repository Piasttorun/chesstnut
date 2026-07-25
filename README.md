# Chesstnut

A local two-player chess app: a Rust chess engine driving a [Tauri](https://tauri.app/) desktop window with a plain HTML/CSS/JS board — no game framework, no frontend build step.

## Features

- Full legal move generation, including check/pin detection, castling (both sides, with all the usual restrictions), en passant, and pawn promotion with a choice of piece
- Checkmate, stalemate, fifty-move-rule, and threefold-repetition detection — the game stops accepting moves once any of these is reached
- Click-to-select, click-to-move board with legal-move highlighting
- New Game button to reset at any time

Not included (by design, for now): move history/undo, saving or loading a game, and there's no AI/engine opponent — it's pass-and-play between two people at the same machine.

## Project layout

```
chesstnut/
├── engine/           # Chess engine — a Rust library crate, no UI dependencies
│   ├── src/engine/
│   │   ├── board.rs  # Board/Square/Piece representation
│   │   ├── moves.rs  # The Move type
│   │   ├── pieces.rs # Per-piece pseudo-legal move generation
│   │   ├── rules.rs  # Check detection, legal-move filtering, promotion
│   │   └── game.rs   # Turn tracking, castling rights, en passant, draw rules
│   └── tests/        # Integration tests only — see "Testing" below
├── src-tauri/        # The Tauri desktop app (Rust)
│   └── src/commands.rs  # Tauri commands the frontend calls (new_game, make_move, ...)
└── web/              # Static frontend: index.html, style.css, main.js
    └── pieces/       # Chess piece SVG sprites (see Credits)
```

`engine/` and `src-tauri/` are members of one Cargo workspace (root `Cargo.toml`); `src-tauri` depends on `engine` as a local path dependency.

## Prerequisites

- **Rust**, installed via [rustup](https://rustup.rs/) (not your distro's package manager — Tauri's tooling needs a fairly current Rust, newer than what e.g. Ubuntu ships)
- **Tauri CLI**: `cargo install tauri-cli --version "^2.0.0" --locked`
- **Linux/WSL system packages** (skip on Windows/macOS, where Tauri uses the OS's built-in webview instead):
  ```
  sudo apt-get install libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```

No Node.js/npm is required — the frontend is plain static files, no bundler.

## Running it

From the repository root:

```bash
cargo tauri dev
```

This builds the engine, the Tauri app, and opens the game window. Edits to `web/*` need the process restarted to take effect (there's no frontend dev server/hot-reload, since there's no build step to serve).

### If you're building on a Windows-mounted drive from WSL

Cargo's heavy parallel rebuilds can hit intermittent `Permission denied` errors when the build output lives on a `/mnt/c/...` path (WSL's bridge to the Windows filesystem doesn't handle concurrent file replacement the way native Linux does). Point `CARGO_TARGET_DIR` at somewhere on the native Linux filesystem instead — source can stay on the Windows drive, only the build output needs to move:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target"
cargo tauri dev
```

## Testing

```bash
cargo test -p chesstnut     # engine only — fast, no Tauri/GUI dependencies
cargo test                  # everything in the workspace
```

Tests live in `engine/tests/`, never inline with the implementation. There's a dedicated `engine/tests/edge_cases.rs` covering the rulebook's tricky corners specifically — castling restrictions, en passant's discovered-check pitfall, double check, and draw-clock resets — worth reading if you want a sense of what the engine actually guarantees.

## Credits

Chess piece sprites (`web/pieces/*.svg`) are the Cburnett set, © Colin M.L. Burnett, licensed under [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/), sourced from [lichess.org](https://github.com/lichess-org/lila).

## License

MIT — see [LICENSE](LICENSE).
