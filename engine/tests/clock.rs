use std::thread;
use std::time::Duration;

use chesstnut::engine::board::{Color, Square};
use chesstnut::engine::game::{Game, GameStatus};
use chesstnut::engine::moves::Move;

#[test]
fn make_move_is_blocked_until_a_time_control_is_chosen() {
    let mut game = Game::new_pending_clock();
    assert!(game.awaiting_clock_choice());

    let result = game.make_move(Move::new(Square::new(4, 1), Square::new(4, 3)));
    assert!(result.is_err());
}

#[test]
fn choosing_no_clock_unblocks_moves_and_reports_no_remaining_time() {
    let mut game = Game::new_pending_clock();
    game.select_time_control(None, 0);
    assert!(!game.awaiting_clock_choice());
    assert!(!game.is_clock_enabled());
    assert_eq!(game.remaining_ms(Color::White), None);

    let result = game.make_move(Move::new(Square::new(4, 1), Square::new(4, 3)));
    assert!(result.is_ok());
}

#[test]
fn untimed_games_report_no_remaining_time() {
    let game = Game::new();
    assert_eq!(game.remaining_ms(Color::White), None);
    assert_eq!(game.remaining_ms(Color::Black), None);
}

#[test]
fn choosing_a_clock_starts_both_sides_at_the_initial_time() {
    let mut game = Game::new_pending_clock();
    game.select_time_control(Some(60_000), 0);

    assert!(game.is_clock_enabled());
    assert_eq!(game.remaining_ms(Color::Black), Some(60_000));
    // White is to move, so its remaining time ticks live from the moment
    // the clock started rather than staying pinned at the initial value.
    assert!(game.remaining_ms(Color::White).unwrap() <= 60_000);
}

#[test]
fn a_move_credits_the_increment_to_the_mover() {
    let mut game = Game::new_pending_clock();
    game.select_time_control(Some(5_000), 1_000);

    game.make_move(Move::new(Square::new(4, 1), Square::new(4, 3))).unwrap(); // e4

    // White spent a negligible amount of real time making this move, so its
    // remaining time should now sit above the 5s start thanks to the 1s
    // increment.
    assert!(game.remaining_ms(Color::White).unwrap() > 5_000);
    // Black's clock hasn't started ticking down from anything but is now
    // the one running.
    assert!(game.remaining_ms(Color::Black).unwrap() <= 5_000);
}

#[test]
fn a_clock_running_out_flags_the_side_to_move() {
    let mut game = Game::new_pending_clock();
    game.select_time_control(Some(30), 0); // 30ms — flags almost immediately
    thread::sleep(Duration::from_millis(80));

    assert_eq!(game.status(), GameStatus::Timeout);
    assert!(game.is_game_over());
    assert_eq!(game.remaining_ms(Color::White), Some(0));

    let result = game.make_move(Move::new(Square::new(4, 1), Square::new(4, 3)));
    assert!(result.is_err());
}

#[test]
fn import_pgn_leaves_the_clock_log_empty_without_clk_comments() {
    let game = Game::import_pgn("1. e4 e5 2. Nf3 Nc6").unwrap();
    assert_eq!(game.move_clock_log(), &[None, None, None, None]);
}

#[test]
fn import_pgn_logs_clk_comments_per_move() {
    let pgn = "1. e4 { [%clk 0:05:00] } e5 { [%clk 0:04:58] } 2. Nf3 Nc6";
    let game = Game::import_pgn(pgn).unwrap();
    assert_eq!(
        game.move_clock_log(),
        &[Some(300_000), Some(298_000), None, None]
    );
}
