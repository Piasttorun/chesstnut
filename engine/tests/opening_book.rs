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
    // Every BOOK_LINES entry matches the (empty) starting position, so
    // book_move picks uniformly among their distinct first moves rather
    // than any single one deterministically — see
    // book_move_varies_its_choice_among_overlapping_lines below for the
    // randomization itself.
    let known_first_moves = [
        Move::new(Square::new(4, 1), Square::new(4, 3)),  // e2-e4
        Move::new(Square::new(3, 1), Square::new(3, 3)),  // d2-d4
        Move::new(Square::new(2, 1), Square::new(2, 3)),  // c2-c4
        Move::new(Square::new(6, 0), Square::new(5, 2)),  // g1-f3
    ];
    let game = Game::new();
    let mv = book_move(&game).expect("the starting position is in book");
    assert!(known_first_moves.contains(&mv), "unexpected book move: {mv:?}");
}

#[test]
fn book_move_continues_a_line_that_has_been_partially_played() {
    // 1. e4 e5 matches the Italian Game, Ruy Lopez and Scotch Game (all
    // continuing 2. Nf3) as well as the Vienna Game (2. Nc3) — either is a
    // legitimate book continuation.
    let known_continuations = [
        Move::new(Square::new(6, 0), Square::new(5, 2)), // g1-f3
        Move::new(Square::new(1, 0), Square::new(2, 2)), // b1-c3
    ];
    let mut game = Game::new();
    play(&mut game, Square::new(4, 1), Square::new(4, 3)); // e2e4
    play(&mut game, Square::new(4, 6), Square::new(4, 4)); // e7e5

    let mv = book_move(&game).expect("still in book after 1. e4 e5");
    assert!(known_continuations.contains(&mv), "unexpected book move: {mv:?}");
}

/// Regression test for the random-choice behavior itself: without it, this
/// would always return the same one of the several equally-valid replies
/// to 1. e4 e5 (whichever line happened to be listed first in
/// BOOK_LINES), which is exactly the bug that made the engine play an
/// identical opening every game. 200 draws from a 2-way uniform choice
/// landing on the same value every time has probability 2 * 0.5^200 —
/// indistinguishable from zero, so this isn't meaningfully flaky.
#[test]
fn book_move_varies_its_choice_among_overlapping_lines() {
    let mut game = Game::new();
    play(&mut game, Square::new(4, 1), Square::new(4, 3)); // e2e4
    play(&mut game, Square::new(4, 6), Square::new(4, 4)); // e7e5

    let mut seen: Vec<Move> = Vec::new();
    for _ in 0..200 {
        let mv = book_move(&game).expect("still in book");
        if !seen.contains(&mv) {
            seen.push(mv);
        }
    }
    assert!(seen.len() > 1, "book_move never varied its choice across 200 calls: {seen:?}");
}

#[test]
fn book_move_returns_none_once_the_game_leaves_every_known_line() {
    let mut game = Game::new();
    // 1. a4 isn't the start of anything in BOOK_LINES.
    play(&mut game, Square::new(0, 1), Square::new(0, 3)); // a2a4
    assert_eq!(book_move(&game), None);
}

/// Regression test for a reported bug: the London System (an extremely
/// common club-level opening) wasn't in the book at all, so a real game
/// that went into it fell out of book after White's very first non-book
/// move and had to search cold from an early, high-branching-factor
/// opening position — exactly the position type search is least equipped
/// to handle quickly (few forcing lines for alpha-beta to cut off on, and
/// none of the endgame/middlegame benchmark positions this engine is
/// otherwise tuned against resemble it). Confirms the book now recognizes
/// the London well past White's first two moves, not just the opening
/// move itself.
#[test]
fn book_move_recognizes_the_london_system_several_moves_deep() {
    let mut game = Game::new();
    play(&mut game, Square::new(3, 1), Square::new(3, 3)); // d2d4
    play(&mut game, Square::new(3, 6), Square::new(3, 4)); // d7d5
    play(&mut game, Square::new(2, 0), Square::new(5, 3)); // c1f4 (Bf4)
    play(&mut game, Square::new(6, 7), Square::new(5, 5)); // g8f6
    play(&mut game, Square::new(4, 1), Square::new(4, 2)); // e2e3
    play(&mut game, Square::new(4, 6), Square::new(4, 5)); // e7e6
    play(&mut game, Square::new(6, 0), Square::new(5, 2)); // g1f3
    play(&mut game, Square::new(5, 7), Square::new(3, 5)); // f8d6 (Bd6)

    let mv = book_move(&game).expect("still in book several moves into the London System");
    assert_eq!(mv, Move::new(Square::new(5, 3), Square::new(6, 2))); // Bf4-g3
}

#[test]
fn best_move_prefers_the_book_over_searching_from_the_starting_position() {
    // Depth 1 alone wouldn't reliably pick one of these on material
    // grounds (a quiet opening position scores 0 either way for most of
    // them) — this confirms best_move is actually consulting the book,
    // not coincidentally agreeing with it.
    let known_first_moves = [
        Move::new(Square::new(4, 1), Square::new(4, 3)), // e2-e4
        Move::new(Square::new(3, 1), Square::new(3, 3)), // d2-d4
        Move::new(Square::new(2, 1), Square::new(2, 3)), // c2-c4
        Move::new(Square::new(6, 0), Square::new(5, 2)), // g1-f3
    ];
    let game = Game::new();
    let mv = best_move(&game, 1).expect("a move exists");
    assert!(known_first_moves.contains(&mv), "unexpected book move: {mv:?}");
}
