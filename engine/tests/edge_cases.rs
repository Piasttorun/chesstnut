//! Regression tests for well-known tricky chess rules — the kind of
//! scenarios a naive move generator gets wrong: castling restrictions,
//! en passant's discovered-check pitfall, double check, and draw-clock
//! resets.

use chesstnut::engine::board::{Board, Color, PieceKind, Square};
use chesstnut::engine::game::{Game, GameStatus, IllegalMove};
use chesstnut::engine::moves::Move;

mod common;
use common::{black_shuffle_start, black_step_square, place, shuffle_round, white_shuffle_start, white_step_square};

// ---------- castling ----------

#[test]
fn cannot_castle_after_king_has_moved_and_returned() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(7, 0), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    let mut game = Game::from_board(board, Color::White);

    game.make_move(Move::new(Square::new(4, 0), Square::new(4, 1)))
        .unwrap(); // Ke1-e2
    game.make_move(Move::new(Square::new(4, 7), Square::new(4, 6)))
        .unwrap(); // Ke8-e7
    game.make_move(Move::new(Square::new(4, 1), Square::new(4, 0)))
        .unwrap(); // Ke2-e1
    game.make_move(Move::new(Square::new(4, 6), Square::new(4, 7)))
        .unwrap(); // Ke7-e8

    let moves = game.legal_moves_from(Square::new(4, 0));
    assert!(!moves.contains(&Move::new(Square::new(4, 0), Square::new(6, 0))));
}

#[test]
fn cannot_castle_while_in_check() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(7, 0), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::Rook, Color::Black);
    place(&mut board, Square::new(0, 7), PieceKind::King, Color::Black);

    let game = Game::from_board(board, Color::White);
    assert_eq!(game.status(), GameStatus::Check);

    let moves = game.legal_moves_from(Square::new(4, 0));
    assert!(!moves.contains(&Move::new(Square::new(4, 0), Square::new(6, 0))));
}

#[test]
fn cannot_castle_queenside_through_attacked_square() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(0, 0), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    place(&mut board, Square::new(3, 7), PieceKind::Rook, Color::Black); // d8 attacks d1

    let game = Game::from_board(board, Color::White);
    let moves = game.legal_moves_from(Square::new(4, 0));
    assert!(!moves.contains(&Move::new(Square::new(4, 0), Square::new(2, 0))));
}

#[test]
fn cannot_castle_kingside_into_an_attacked_landing_square() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(7, 0), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    place(&mut board, Square::new(6, 7), PieceKind::Rook, Color::Black); // g8 attacks g1

    let game = Game::from_board(board, Color::White);
    let moves = game.legal_moves_from(Square::new(4, 0));
    assert!(!moves.contains(&Move::new(Square::new(4, 0), Square::new(6, 0))));
}

#[test]
fn cannot_castle_kingside_when_path_blocked_by_own_piece() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(7, 0), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(5, 0), PieceKind::Bishop, Color::White); // f1
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);

    let game = Game::from_board(board, Color::White);
    let moves = game.legal_moves_from(Square::new(4, 0));
    assert!(!moves.contains(&Move::new(Square::new(4, 0), Square::new(6, 0))));
}

#[test]
fn cannot_castle_queenside_when_b_file_square_is_occupied() {
    // b1 isn't a square the king passes through, but the rook still needs a
    // clear path to get there.
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(0, 0), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(1, 0), PieceKind::Knight, Color::White); // b1
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);

    let game = Game::from_board(board, Color::White);
    let moves = game.legal_moves_from(Square::new(4, 0));
    assert!(!moves.contains(&Move::new(Square::new(4, 0), Square::new(2, 0))));
}

#[test]
fn cannot_castle_when_rook_is_missing() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);

    let game = Game::from_board(board, Color::White);
    let moves = game.legal_moves_from(Square::new(4, 0));
    assert!(!moves.contains(&Move::new(Square::new(4, 0), Square::new(6, 0))));
    assert!(!moves.contains(&Move::new(Square::new(4, 0), Square::new(2, 0))));
}

// ---------- en passant ----------

#[test]
fn en_passant_not_available_from_a_non_adjacent_file() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    place(&mut board, Square::new(1, 4), PieceKind::Pawn, Color::White); // b5, not adjacent to d
    place(&mut board, Square::new(3, 6), PieceKind::Pawn, Color::Black); // d7

    let mut game = Game::from_board(board, Color::Black);
    game.make_move(Move::new(Square::new(3, 6), Square::new(3, 4)))
        .unwrap(); // d7-d5

    let moves = game.legal_moves_from(Square::new(1, 4));
    assert!(!moves.contains(&Move::new(Square::new(1, 4), Square::new(3, 5))));
}

/// The classic en passant trap: capturing removes *both* pawns from the rank
/// at once, which can open a line to the king that neither pawn alone was
/// blocking. Naive engines that don't simulate the capture for check-safety
/// get this wrong.
#[test]
fn en_passant_illegal_if_it_exposes_king_to_horizontal_check() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 4), PieceKind::King, Color::White); // e5
    place(&mut board, Square::new(3, 4), PieceKind::Pawn, Color::White); // d5
    place(&mut board, Square::new(2, 6), PieceKind::Pawn, Color::Black); // c7
    place(&mut board, Square::new(0, 4), PieceKind::Rook, Color::Black); // a5
    place(&mut board, Square::new(7, 7), PieceKind::King, Color::Black); // h8

    let mut game = Game::from_board(board, Color::Black);
    game.make_move(Move::new(Square::new(2, 6), Square::new(2, 4)))
        .unwrap(); // c7-c5

    let moves = game.legal_moves_from(Square::new(3, 4));
    assert!(!moves.contains(&Move::new(Square::new(3, 4), Square::new(2, 5)))); // dxc6 e.p.
}

// ---------- check / pins / double check ----------

#[test]
fn king_cannot_capture_a_defended_piece() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White); // e1
    place(&mut board, Square::new(4, 1), PieceKind::Knight, Color::Black); // e2
    place(&mut board, Square::new(4, 7), PieceKind::Rook, Color::Black); // e8, defends e2
    place(&mut board, Square::new(0, 7), PieceKind::King, Color::Black);

    let game = Game::from_board(board, Color::White);
    let moves = game.legal_moves_from(Square::new(4, 0));
    assert!(!moves.contains(&Move::new(Square::new(4, 0), Square::new(4, 1))));
}

#[test]
fn double_check_only_the_king_can_respond() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White); // e1
    place(&mut board, Square::new(2, 2), PieceKind::Bishop, Color::White); // c3
    place(&mut board, Square::new(4, 7), PieceKind::Rook, Color::Black); // e8: checks via file
    place(&mut board, Square::new(3, 2), PieceKind::Knight, Color::Black); // d3: checks directly
    place(&mut board, Square::new(0, 7), PieceKind::King, Color::Black);

    let game = Game::from_board(board, Color::White);
    assert_eq!(game.status(), GameStatus::Check);

    // c3-e5 would block the rook's file check, but the knight's check is
    // independent of that line — blocking one checker while the other still
    // has the king is still illegal.
    assert!(game.legal_moves_from(Square::new(2, 2)).is_empty());
}

#[test]
fn blocking_a_single_check_is_legal() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White); // e1
    place(&mut board, Square::new(0, 3), PieceKind::Rook, Color::White); // a4
    place(&mut board, Square::new(4, 7), PieceKind::Rook, Color::Black); // e8
    place(&mut board, Square::new(0, 7), PieceKind::King, Color::Black);

    let game = Game::from_board(board, Color::White);
    let moves = game.legal_moves_from(Square::new(0, 3));
    assert!(moves.contains(&Move::new(Square::new(0, 3), Square::new(4, 3)))); // Ra4-e4
}

// ---------- promotion ----------

#[test]
fn promotion_by_capture_reaches_the_back_rank() {
    let mut board = Board::empty();
    place(&mut board, Square::new(1, 6), PieceKind::Pawn, Color::White); // b7
    place(&mut board, Square::new(0, 7), PieceKind::Rook, Color::Black); // a8
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);

    let game = Game::from_board(board, Color::White);
    let moves = game.legal_moves_from(Square::new(1, 6));
    assert!(moves.contains(&Move::promotion(
        Square::new(1, 6),
        Square::new(0, 7),
        PieceKind::Queen
    )));
}

#[test]
fn all_four_promotion_choices_are_legal_when_not_in_check() {
    let mut board = Board::empty();
    place(&mut board, Square::new(0, 6), PieceKind::Pawn, Color::White); // a7
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);

    let game = Game::from_board(board, Color::White);
    let moves = game.legal_moves_from(Square::new(0, 6));
    let to = Square::new(0, 7);

    for kind in [
        PieceKind::Queen,
        PieceKind::Rook,
        PieceKind::Bishop,
        PieceKind::Knight,
    ] {
        assert!(moves.contains(&Move::promotion(Square::new(0, 6), to, kind)));
    }
}

// ---------- draw clocks ----------

#[test]
fn fifty_move_counter_resets_on_capture() {
    let mut board = Board::empty();
    place(&mut board, white_shuffle_start(), PieceKind::King, Color::White);
    place(&mut board, black_shuffle_start(), PieceKind::King, Color::Black);
    // g4/g3 — clear of both shuffle loops (white: files 0-5 ranks 4-5;
    // black: files 0-3 ranks 0-1).
    place(&mut board, Square::new(6, 3), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(6, 2), PieceKind::Rook, Color::Black);
    let mut game = Game::from_board(board, Color::White);

    let mut white_step = 0usize;
    let mut black_step = 0usize;

    for _ in 0..30 {
        shuffle_round(&mut game, &mut white_step, &mut black_step);
    }
    assert_eq!(game.status(), GameStatus::InProgress); // 60 half-moves, clock not full

    // A capture partway through (replacing white's shuffle move for this
    // round) must reset the clock.
    game.make_move(Move::new(Square::new(6, 3), Square::new(6, 2)))
        .unwrap(); // Rxg3
    let black_from = black_step_square(black_step);
    black_step += 1;
    let black_to = black_step_square(black_step);
    game.make_move(Move::new(black_from, black_to)).unwrap();

    for _ in 0..30 {
        shuffle_round(&mut game, &mut white_step, &mut black_step);
    }

    // 122 half-moves played in total (well past 100), but only 61 since the
    // capture — a naive un-reset counter would have already called this a
    // draw much earlier.
    assert_eq!(game.status(), GameStatus::InProgress);
}

// ---------- game-over enforcement ----------

#[test]
fn fifty_move_draw_rejects_further_moves() {
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

    // The king can still physically shuffle — the game must refuse it anyway.
    let white_from = white_step_square(white_step);
    let white_to = white_step_square(white_step + 1);
    let result = game.make_move(Move::new(white_from, white_to));
    assert_eq!(result, Err(IllegalMove));
}
