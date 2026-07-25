use chesstnut::engine::board::{Color, Square};
use chesstnut::engine::game::{Game, GameStatus};
use chesstnut::engine::moves::Move;

#[test]
fn resigning_ends_the_game_for_the_side_to_move() {
    let mut game = Game::new();
    assert_eq!(game.turn(), Color::White);

    game.resign().unwrap();

    assert_eq!(game.status(), GameStatus::Resignation);
    assert!(game.is_game_over());
    // The resigning side is whoever was to move — same convention as
    // checkmate, where callers read the winner off of `turn()`.
    assert_eq!(game.turn(), Color::White);
}

#[test]
fn no_further_moves_are_accepted_after_resigning() {
    let mut game = Game::new();
    game.resign().unwrap();

    let result = game.make_move(Move::new(Square::new(4, 1), Square::new(4, 3)));
    assert!(result.is_err());
}

#[test]
fn resigning_a_game_that_already_ended_fails() {
    let mut game = Game::new();
    game.resign().unwrap();

    let result = game.resign();
    assert!(result.is_err());
}

#[test]
fn resigning_after_checkmate_fails() {
    // Fool's mate — Black delivers checkmate on move 2.
    let mut game = Game::new();
    game.make_move(Move::new(Square::new(5, 1), Square::new(5, 2))).unwrap(); // f3
    game.make_move(Move::new(Square::new(4, 6), Square::new(4, 4))).unwrap(); // e5
    game.make_move(Move::new(Square::new(6, 1), Square::new(6, 3))).unwrap(); // g4
    game.make_move(Move::new(Square::new(3, 7), Square::new(7, 3))).unwrap(); // Qh4#

    assert_eq!(game.status(), GameStatus::Checkmate);

    let result = game.resign();
    assert!(result.is_err());
    assert_eq!(game.status(), GameStatus::Checkmate);
}

#[test]
fn black_can_resign_on_its_own_turn() {
    let mut game = Game::new();
    game.make_move(Move::new(Square::new(4, 1), Square::new(4, 3))).unwrap(); // e4
    assert_eq!(game.turn(), Color::Black);

    game.resign().unwrap();

    assert_eq!(game.status(), GameStatus::Resignation);
    assert_eq!(game.turn(), Color::Black);
}

#[test]
fn resigning_still_works_when_a_clock_is_running() {
    let mut game = Game::new_pending_clock();
    game.select_time_control(Some(60_000), 0);

    game.resign().unwrap();

    assert_eq!(game.status(), GameStatus::Resignation);
    assert!(game.is_game_over());
}
