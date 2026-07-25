use chesstnut::ai::{best_move, book_move};
use chesstnut::engine::board::Square;
use chesstnut::engine::game::Game;
use chesstnut::engine::moves::Move;

fn play(game: &mut Game, from: Square, to: Square) {
    let mv = game
        .legal_moves_from(from)
        .into_iter()
        .find(|mv| mv.to == to)
        .expect("move should be legal");
    game.make_move(mv).expect("move should apply");
}

#[test]
fn book_move_opens_with_a_known_line_from_the_starting_position() {
    let game = Game::new();
    let mv = book_move(&game).expect("the starting position is in book");
    // The first BOOK_LINES entry to match an empty history wins — that's
    // the Italian Game, opening 1. e4.
    assert_eq!(mv, Move::new(Square::new(4, 1), Square::new(4, 3))); // e2-e4
}

#[test]
fn book_move_continues_a_line_that_has_been_partially_played() {
    let mut game = Game::new();
    play(&mut game, Square::new(4, 1), Square::new(4, 3)); // e2e4
    play(&mut game, Square::new(4, 6), Square::new(4, 4)); // e7e5

    let mv = book_move(&game).expect("still in book after 1. e4 e5");
    assert_eq!(mv, Move::new(Square::new(6, 0), Square::new(5, 2))); // g1-f3
}

#[test]
fn book_move_returns_none_once_the_game_leaves_every_known_line() {
    let mut game = Game::new();
    // 1. a4 isn't the start of anything in BOOK_LINES.
    play(&mut game, Square::new(0, 1), Square::new(0, 3)); // a2a4
    assert_eq!(book_move(&game), None);
}

#[test]
fn best_move_prefers_the_book_over_searching_from_the_starting_position() {
    // Depth 1 alone wouldn't reliably pick 1. e4 on material grounds (a
    // quiet opening position scores 0 either way) — this confirms
    // best_move is actually consulting the book, not coincidentally
    // agreeing with it.
    let game = Game::new();
    let mv = best_move(&game, 1).expect("a move exists");
    assert_eq!(mv, Move::new(Square::new(4, 1), Square::new(4, 3))); // e2-e4
}
