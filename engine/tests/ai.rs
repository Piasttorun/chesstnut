use chesstnut::ai::{evaluate, Engine, RandomEngine, Score};
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

#[test]
fn starting_position_is_materially_even() {
    let board = Board::starting_position();
    assert_eq!(evaluate(&board), Score::Centipawns(0));
}

#[test]
fn evaluate_favors_white_up_a_queen() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    // b2 rather than the queen's own starting square (d1) — the
    // piece-square table gives d1 a small penalty, which would make this
    // a test of the table instead of a test of material counting.
    place(&mut board, Square::new(1, 1), PieceKind::Queen, Color::White);

    assert_eq!(evaluate(&board), Score::Centipawns(900));
}

#[test]
fn evaluate_prefers_a_centralized_knight_over_a_rim_knight() {
    let mut white_board = Board::empty();
    place(&mut white_board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut white_board, Square::new(4, 7), PieceKind::King, Color::Black);
    place(&mut white_board, Square::new(4, 4), PieceKind::Knight, Color::White);

    let mut rim_board = Board::empty();
    place(&mut rim_board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut rim_board, Square::new(4, 7), PieceKind::King, Color::Black);
    place(&mut rim_board, Square::new(0, 0), PieceKind::Knight, Color::White);

    let Score::Centipawns(centralized) = evaluate(&white_board) else {
        panic!("expected a centipawn score");
    };
    let Score::Centipawns(rim) = evaluate(&rim_board) else {
        panic!("expected a centipawn score");
    };
    assert!(centralized > rim, "centralized knight ({centralized}) should score higher than a rim knight ({rim})");
}

#[test]
fn evaluate_favors_black_up_a_rook() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    place(&mut board, Square::new(0, 7), PieceKind::Rook, Color::Black);

    assert_eq!(evaluate(&board), Score::Centipawns(-500));
}
