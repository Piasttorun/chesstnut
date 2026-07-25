use std::time::Instant;

use chesstnut::ai::{best_move, search};
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
