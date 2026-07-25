# Chesstnut

A chess app: a Rust chess engine — including a from-scratch AI opponent — driving a [Tauri](https://tauri.app/) desktop window, with a plain HTML/CSS/JS board. No game framework, no frontend build step, no Node/npm.

## Features

**Rules & board**
- Full legal move generation, including check/pin detection, castling (both sides, with all the usual restrictions), en passant, and pawn promotion with a choice of piece
- Checkmate, stalemate, fifty-move-rule, and threefold-repetition detection — the game stops accepting moves once any of these is reached
- Click-to-select or drag-and-drop (your choice, in Settings), with legal-move highlighting and a slide-in animation on the move that just happened
- Time controls — bullet/blitz/rapid/classical presets or untimed — plus Resign
- FEN and PGN: view, copy, and load/import at any time; the game-over screen pre-fills both for easy sharing
- Move/capture/check/game-over sound effects (mutable)

**AI opponent**
- Play against another human, or against the engine — pick a side and a strength (search depth) before the game starts; the board flips automatically when you play Black
- The engine: alpha-beta search in negamax form, iterative deepening, quiescence search at the leaves (so it doesn't stop mid-capture), root-move parallelism across available CPU cores, and a cancellation mechanism so a stale search can never block input or clobber a fresher one
- Evaluation is material plus piece-square tables (centralized knights, advanced pawns, king safety, etc.) — not just a material count
- A small hand-curated opening book so early moves reflect known theory instantly rather than re-deriving it move by move
- A live evaluation bar (independent search depth, configurable) showing the current score or a forced mate, usable with or without an AI opponent in the game

**Analysis**
- A separate sandbox board — its own independent game, untouched by whatever's happening on the Play tab — with no clock, no opponent, and the eval bar always on
- Reset Position and Flip Board controls, plus PGN import for studying an existing game

**Interface**
- Three tabs: **Home** (landing page), **Play** (everything above), **Analysis** (the sandbox above)
- A settings widget for interaction mode (drag/click) and mute, persisted across restarts

Not included (by design, for now): move undo, saved game history beyond FEN/PGN round-tripping, network play, and a transposition table (the next lever for search speed once one's needed).

## Project layout

```
chesstnut/
├── engine/                    # Chess engine — a Rust library crate, no UI dependencies
│   ├── src/
│   │   ├── engine/
│   │   │   ├── board.rs       # Board/Square/Piece representation
│   │   │   ├── moves.rs       # The Move type
│   │   │   ├── pieces.rs      # Per-piece pseudo-legal move generation
│   │   │   ├── rules.rs       # Check detection, legal-move filtering, promotion
│   │   │   ├── game.rs        # Turn tracking, castling rights, en passant, draw rules, clocks
│   │   │   ├── fen.rs         # FEN parsing/export
│   │   │   └── pgn.rs         # SAN move text and PGN import/export
│   │   └── ai/
│   │       ├── eval.rs            # Material + piece-square-table evaluation
│   │       ├── search.rs          # Alpha-beta, iterative deepening, quiescence, cancellation
│   │       ├── opening_book.rs    # Hand-curated opening lines
│   │       └── random.rs          # RandomEngine — a legitimate "easy" difficulty
│   └── tests/                 # Integration tests only — see "Testing" below
├── src-tauri/                  # The Tauri desktop app (Rust)
│   └── src/commands.rs         # Tauri commands the frontend calls: make_move, analyze,
│                                # request_ai_move, resign, load_fen/load_pgn, and their
│                                # analysis_* twins for the independent Analysis board
├── web/                         # Static frontend: index.html, style.css, main.js
│   └── pieces/                   # Chess piece SVG sprites (see Credits)
└── launch-chesstnut.bat         # Double-click launcher (Windows) — see "Running it"
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

This builds the engine, the Tauri app, and opens the game window. The dev command watches `engine/` and `src-tauri/` and rebuilds automatically on changes there; `web/*` isn't watched (it's plain static files with no build step), so a JS/HTML/CSS edit just needs the app window reloaded (Ctrl+R) to show up — no rebuild or restart required.

**On Windows**, double-clicking `launch-chesstnut.bat` does the same thing without typing anything — it just forwards into WSL. If you'd rather trigger it from VS Code, the [Code Runner](https://marketplace.visualstudio.com/items?itemName=formulahendry.code-runner) extension adds a right-click → "Run Code" option for it.

Search performance in dev builds specifically gets a boost from a per-package override in the root `Cargo.toml` (`[profile.dev.package.chesstnut] opt-level = 3`) — without it, the engine crate alone ran roughly 10x slower under `cargo tauri dev`'s otherwise-unoptimized dev profile, which was enough to make deep searches noticeably laggy.

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

Tests live in `engine/tests/`, never inline with the implementation. `engine/tests/edge_cases.rs` covers the rulebook's tricky corners specifically — castling restrictions, en passant's discovered-check pitfall, double check, and draw-clock resets — worth reading if you want a sense of what the engine actually guarantees.

`engine/tests/search_bench.rs` holds performance benchmarks rather than correctness tests — they measure wall-clock time, so they're `#[ignore]`d by default and need to be run explicitly, in release mode, one at a time (Rust's test runner parallelizes `#[test]` functions by default, which would have two benchmarks silently compete for the same CPU cores):

```bash
cargo test --release -p chesstnut --test search_bench -- \
  bench_opening_move_depth bench_middlegame_move_depth bench_endgame_move_depth \
  --ignored --nocapture --test-threads=1
```

## Credits

Chess piece sprites (`web/pieces/*.svg`) are the Cburnett set, © Colin M.L. Burnett, licensed under [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/), sourced from [lichess.org](https://github.com/lichess-org/lila).

## License

MIT — see [LICENSE](LICENSE).
