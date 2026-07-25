use chesstnut::ai::{Engine, RandomEngine};
use chesstnut::engine::board::{Board, Color, Piece, PieceKind, Square};
use chesstnut::engine::game::Game;

fn place(board: &mut Board, square: Square, kind: PieceKind, color: Color) {
    board.set(square, Some(Piece { kind, color }));
}

#[test]
fn random_engine_always_picks_a_legal_move() {
    let game = Game::new();
    let engine = RandomEngine;

    // Run it a bunch of times rather than once — a random choice could
    // land on a valid move by luck even if the selection logic were
    // broken, so this leans on repetition to catch that.
    for _ in 0..50 {
        let mv = engine.choose_move(&game).expect("opening position has legal moves");
        assert!(game.legal_moves_for_turn().contains(&mv));
    }
}

#[test]
fn random_engine_returns_none_when_stalemated() {
    let mut board = Board::empty();
    place(&mut board, Square::new(7, 7), PieceKind::King, Color::Black);
    place(&mut board, Square::new(5, 6), PieceKind::King, Color::White);
    place(&mut board, Square::new(6, 5), PieceKind::Queen, Color::White);
    let game = Game::from_board(board, Color::Black);
    let engine = RandomEngine;

    assert_eq!(engine.choose_move(&game), None);
}
