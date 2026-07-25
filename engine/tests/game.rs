use chesstnut::engine::board::{Board, Color, PieceKind, Square};
use chesstnut::engine::game::{Game, GameStatus, IllegalMove};
use chesstnut::engine::moves::Move;

mod common;
use common::{black_shuffle_start, place, shuffle_round, white_shuffle_start};

#[test]
fn new_game_starts_with_white_to_move_and_in_progress() {
    let game = Game::new();
    assert_eq!(game.turn(), Color::White);
    assert_eq!(game.status(), GameStatus::InProgress);
}

#[test]
fn legal_move_succeeds_and_switches_turn() {
    let mut game = Game::new();
    let result = game.make_move(Move::new(Square::new(4, 1), Square::new(4, 3)));

    assert!(result.is_ok());
    assert_eq!(game.turn(), Color::Black);
    assert_eq!(
        game.board().get(Square::new(4, 3)).unwrap().kind,
        PieceKind::Pawn
    );
}

#[test]
fn moving_out_of_turn_is_rejected() {
    let mut game = Game::new();
    let result = game.make_move(Move::new(Square::new(4, 6), Square::new(4, 4)));

    assert_eq!(result, Err(IllegalMove));
    assert_eq!(game.turn(), Color::White);
}

#[test]
fn illegal_pawn_jump_is_rejected() {
    let mut game = Game::new();
    let result = game.make_move(Move::new(Square::new(4, 1), Square::new(4, 5)));

    assert_eq!(result, Err(IllegalMove));
}

#[test]
fn checkmate_status_is_detected() {
    let mut board = Board::empty();
    place(&mut board, Square::new(0, 7), PieceKind::King, Color::Black);
    place(&mut board, Square::new(0, 6), PieceKind::Queen, Color::White);
    place(&mut board, Square::new(1, 5), PieceKind::King, Color::White);

    let game = Game::from_board(board, Color::Black);
    assert_eq!(game.status(), GameStatus::Checkmate);
}

#[test]
fn castling_available_both_sides_when_path_is_clear_and_safe() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(7, 0), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(0, 0), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);

    let game = Game::from_board(board, Color::White);
    let moves = game.legal_moves_from(Square::new(4, 0));

    assert!(moves.contains(&Move::new(Square::new(4, 0), Square::new(6, 0)))); // kingside
    assert!(moves.contains(&Move::new(Square::new(4, 0), Square::new(2, 0)))); // queenside
}

#[test]
fn castling_blocked_when_path_is_attacked() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(7, 0), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    // Black rook covers f1, a square the king must pass through to castle
    // kingside.
    place(&mut board, Square::new(5, 7), PieceKind::Rook, Color::Black);

    let game = Game::from_board(board, Color::White);
    let moves = game.legal_moves_from(Square::new(4, 0));

    assert!(!moves.contains(&Move::new(Square::new(4, 0), Square::new(6, 0))));
}

#[test]
fn castling_rights_are_lost_permanently_once_the_rook_moves() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(7, 0), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    let mut game = Game::from_board(board, Color::White);

    // Send the rook for a walk and bring it straight back to h1.
    game.make_move(Move::new(Square::new(7, 0), Square::new(7, 3)))
        .unwrap();
    game.make_move(Move::new(Square::new(4, 7), Square::new(4, 6)))
        .unwrap();
    game.make_move(Move::new(Square::new(7, 3), Square::new(7, 0)))
        .unwrap();
    game.make_move(Move::new(Square::new(4, 6), Square::new(4, 7)))
        .unwrap();

    let moves = game.legal_moves_from(Square::new(4, 0));
    assert!(!moves.contains(&Move::new(Square::new(4, 0), Square::new(6, 0))));
}

#[test]
fn en_passant_capture_available_immediately_after_a_double_step() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    place(&mut board, Square::new(4, 4), PieceKind::Pawn, Color::White); // e5
    place(&mut board, Square::new(3, 6), PieceKind::Pawn, Color::Black); // d7

    let mut game = Game::from_board(board, Color::Black);
    game.make_move(Move::new(Square::new(3, 6), Square::new(3, 4)))
        .unwrap(); // d7-d5

    let moves = game.legal_moves_from(Square::new(4, 4));
    assert!(moves.contains(&Move::new(Square::new(4, 4), Square::new(3, 5)))); // exd6 e.p.
}

#[test]
fn en_passant_expires_after_one_move() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(0, 1), PieceKind::King, Color::Black);
    place(&mut board, Square::new(4, 4), PieceKind::Pawn, Color::White); // e5
    place(&mut board, Square::new(3, 6), PieceKind::Pawn, Color::Black); // d7

    let mut game = Game::from_board(board, Color::Black);
    game.make_move(Move::new(Square::new(3, 6), Square::new(3, 4)))
        .unwrap(); // d7-d5
    game.make_move(Move::new(Square::new(4, 0), Square::new(4, 1)))
        .unwrap(); // white plays something else
    game.make_move(Move::new(Square::new(0, 1), Square::new(0, 2)))
        .unwrap(); // black plays something else

    let moves = game.legal_moves_from(Square::new(4, 4));
    assert!(!moves.contains(&Move::new(Square::new(4, 4), Square::new(3, 5))));
}

#[test]
fn fifty_move_rule_triggers_draw() {
    let mut board = Board::empty();
    place(&mut board, white_shuffle_start(), PieceKind::King, Color::White);
    place(&mut board, black_shuffle_start(), PieceKind::King, Color::Black);
    let mut game = Game::from_board(board, Color::White);

    let mut white_step = 0usize;
    let mut black_step = 0usize;
    for _ in 0..50 {
        shuffle_round(&mut game, &mut white_step, &mut black_step);
    }

    assert_eq!(game.status(), GameStatus::DrawByFiftyMoveRule);
}

#[test]
fn threefold_repetition_triggers_draw() {
    let mut board = Board::empty();
    place(&mut board, Square::new(0, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(7, 7), PieceKind::King, Color::Black);
    let mut game = Game::from_board(board, Color::White);

    let lap = [
        (Square::new(0, 0), Square::new(0, 1)), // white a1-a2
        (Square::new(7, 7), Square::new(7, 6)), // black h8-h7
        (Square::new(0, 1), Square::new(0, 0)), // white a2-a1
        (Square::new(7, 6), Square::new(7, 7)), // black h7-h8
    ];

    // The starting position is occurrence #1. Each full lap back to the
    // start recreates it — two laps brings the total to 3.
    for _ in 0..2 {
        for &(from, to) in &lap {
            game.make_move(Move::new(from, to)).unwrap();
        }
    }

    assert_eq!(game.status(), GameStatus::DrawByRepetition);
}
