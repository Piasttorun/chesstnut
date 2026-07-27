use std::io::Write;
use std::time::{Duration, Instant};

use chesstnut::ai::{best_move, book_move, search};
use chesstnut::engine::board::Square;
use chesstnut::engine::game::Game;

/// Not run by default (`cargo test` skips #[ignore]) — this measures wall
/// time, not correctness, so it doesn't belong in the normal pass/fail
/// suite. Run explicitly with `cargo test --release -p chesstnut --test
/// search_bench -- --ignored --nocapture` whenever a search change (move
/// ordering, iterative deepening, a future transposition table, ...) needs
/// its actual before/after impact checked rather than assumed.
#[test]
#[ignore]
fn bench_search_from_starting_position() {
    // new_pending_clock() starts with no time control chosen yet, which
    // search() special-cases to an instant Centipawns(0) — pick "no clock"
    // first so this actually measures the real search path.
    let mut game = Game::new_pending_clock();
    game.select_time_control(None, 0);

    for depth in 1..=6 {
        let start = Instant::now();
        let score = search(&game, depth);
        println!("depth {depth}: {score:?} in {:?}", start.elapsed());
    }
}

/// Diagnostic for a reported hang: after playing 1. e4, request_ai_move
/// (which calls best_move) apparently never returns. This isolates
/// whether that's a general negamax/quiescence problem (would also show
/// up here, on the plain search/best_move path with no Tauri/IPC/
/// generation-tracking involved at all) or something specific to that
/// plumbing.
#[test]
#[ignore]
fn bench_best_move_after_e4() {
    let mut game = Game::new_pending_clock();
    game.select_time_control(None, 0);

    let e2 = Square::new(4, 1);
    let e4 = Square::new(4, 3);
    let mv = game.legal_moves_from(e2).into_iter().find(|mv| mv.to == e4).expect("e2e4 is legal");
    game.make_move(mv).expect("e2e4 should apply");

    for depth in 1..=6 {
        let start = Instant::now();
        let chosen = best_move(&game, depth);
        println!("depth {depth}: {chosen:?} in {:?}", start.elapsed());
    }
}

// ---------- Practical per-move depth ----------
//
// Answers a concrete question: for a "normal" game (someone actually
// sitting there waiting on each of the engine's moves), what's the
// deepest search that still lands in a tolerable 3-6 second window per
// move? Split into three standardized, independently-runnable tests — one
// per game phase — rather than one combined test looping over an array,
// so a single phase can be re-checked on its own (e.g. only the endgame,
// after a quiescence-search change) without waiting on the other two.
// probe_depths below is the one shared measurement routine all three call,
// so the method (and the 3-6s target, the "unreasonable" cutoff, etc.)
// stays identical across phases — only the position differs.
//
// The opening book is a real, deliberate part of how the engine plays —
// not a shortcut that hides the true cost of a move — so a book-covered
// position is reported as instant and left at that, exactly like a real
// game: `book_move()` is checked first, same as `best_move()` does, and
// the depth loop only runs at all once that returns `None`. This does mean
// bench_opening_move_depth is optimistic if a human opponent goes off-book
// fast (say, 1.b4) — the book only covers a handful of known lines today,
// and widening it is the answer to that, not weakening this benchmark.
//
// Run any one of these with, e.g., `cargo test --release -p chesstnut
// --test search_bench -- bench_middlegame_move_depth --ignored
// --nocapture` — release mode matters here specifically (unlike the two
// benches above, which lean on the per-package dev opt-level override in
// the workspace Cargo.toml): this is about finding the real ceiling
// players will experience, and the shipped app runs `cargo tauri dev`'s
// dev profile, so checking release timings too shows how much headroom an
// eventual release build adds on top.

// Once a single depth takes this long, it's unambiguously past "waiting
// on a normal move" territory (5x the top of the 3-6s target) — no point
// grinding through even deeper, exponentially slower plies just to confirm
// what's already obvious.
const UNREASONABLE_CEILING: Duration = Duration::from_secs(30);
// Far beyond anything reachable in reasonable time even at the fastest
// measured depth — should never actually be hit; exists purely so a bug
// can't turn "keep going until it's slow enough" into "keep going
// forever." (An earlier version of this file called `best_move()` without
// checking the book first, so every depth on a book-covered position
// resolved in a few microseconds — a book lookup, not a search — and the
// time-based cutoff above could never trigger; the loop ran for over a
// million "depths" before anyone noticed. This cap means that class of bug
// can't do that again even if it recurs some other way.)
const HARD_DEPTH_CAP: u32 = 40;
const SWEET_SPOT: std::ops::RangeInclusive<Duration> = Duration::from_secs(3)..=Duration::from_secs(6);

/// Shared by all three phase benchmarks below. Checks the opening book
/// first (see the module doc comment for why that's the correct behavior,
/// not a gap); otherwise searches one ply deeper at a time, printing each
/// depth's timing immediately (not left to Rust's normal per-test output
/// buffering, which would otherwise hold every line back until the whole
/// test finishes) until a depth blows past UNREASONABLE_CEILING or
/// HARD_DEPTH_CAP is reached.
fn probe_depths(label: &str, game: &Game) {
    println!("\n-- {label} --");
    std::io::stdout().flush().unwrap();

    if book_move(game).is_some() {
        println!("(covered by the opening book — engine plays instantly here, no search needed)");
        std::io::stdout().flush().unwrap();
        return;
    }

    for depth in 1..=HARD_DEPTH_CAP {
        let start = Instant::now();
        let score = search(game, depth);
        let elapsed = start.elapsed();
        let note = if SWEET_SPOT.contains(&elapsed) {
            "  <-- within the 3-6s target"
        } else if elapsed > *SWEET_SPOT.end() {
            "  <-- over budget"
        } else {
            ""
        };
        println!("depth {depth}: {score:?} in {elapsed:?}{note}");
        std::io::stdout().flush().unwrap();
        if elapsed > UNREASONABLE_CEILING {
            println!("(stopping here — well past a reasonable per-move wait)");
            std::io::stdout().flush().unwrap();
            break;
        }
    }
}

/// The two most common opening moments: the very first move, and the
/// first reply to 1.e4. Both are covered by the current book, so this is
/// expected to report "instant" for each — that's the point, not a gap in
/// the measurement (see the module doc comment above).
#[test]
#[ignore]
fn bench_opening_move_depth() {
    let mut starting = Game::new_pending_clock();
    starting.select_time_control(None, 0);
    probe_depths("starting position", &starting);

    let mut opening = Game::new_pending_clock();
    opening.select_time_control(None, 0);
    let e2 = Square::new(4, 1);
    let e4 = Square::new(4, 3);
    let mv = opening.legal_moves_from(e2).into_iter().find(|mv| mv.to == e4).expect("e2e4 is legal");
    opening.make_move(mv).expect("e2e4 should apply");
    probe_depths("after 1.e4", &opening);
}

/// A real middlegame at move 16 — both sides castled on opposite wings
/// (white queenside, black kingside), one minor piece traded off each
/// side, but still 15 units per side and every remaining piece active.
/// Well past anything the current book covers, and the realistic
/// worst-case branching factor a full game actually reaches.
#[test]
#[ignore]
fn bench_middlegame_move_depth() {
    const MIDDLEGAME_FEN: &str = "2rq1rk1/pb1n1ppp/1p2pn2/2ppN3/3P4/2P1PN2/PPQ2PPP/2KR1B1R w - - 2 16";
    let middlegame = Game::from_fen(MIDDLEGAME_FEN).expect("valid middlegame FEN");
    probe_depths("middlegame (move 16)", &middlegame);
}

/// A simplified rook-and-pawns endgame — the kind of position a real game
/// actually reaches once most pieces are off, not a contrived near-empty
/// board. Expected to search considerably deeper than the middlegame in
/// the same amount of time, since far fewer legal moves exist at every
/// ply.
#[test]
#[ignore]
fn bench_endgame_move_depth() {
    const ENDGAME_FEN: &str = "5r2/5pk1/6p1/7p/7P/6P1/5PK1/5R2 w - - 0 40";
    let endgame = Game::from_fen(ENDGAME_FEN).expect("valid endgame FEN");
    probe_depths("simplified rook endgame", &endgame);
}
