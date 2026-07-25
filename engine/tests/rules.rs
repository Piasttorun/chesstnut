use chesstnut::engine::board::{Board, Color, Piece, PieceKind, Square};
use chesstnut::engine::moves::Move;
use chesstnut::engine::rules::{is_checkmate, is_in_check, is_stalemate, legal_moves_from};

fn place(board: &mut Board, square: Square, kind: PieceKind, color: Color) {
    board.set(square, Some(Piece { kind, color }));
}

#[test]
fn king_not_in_check_on_open_board() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);

    assert!(!is_in_check(&board, Color::White));
}

#[test]
fn king_in_check_from_clear_rook_file() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::Rook, Color::Black);

    assert!(is_in_check(&board, Color::White));
}

#[test]
fn pinned_rook_cannot_leave_the_pin_line() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(4, 1), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::Rook, Color::Black);

    let moves = legal_moves_from(&board, Square::new(4, 1));

    // Sliding sideways off the e-file would expose the king: illegal.
    assert!(!moves.contains(&Move::new(Square::new(4, 1), Square::new(3, 1))));
    // Staying on the file, including capturing the pinning rook, stays legal.
    assert!(moves.contains(&Move::new(Square::new(4, 1), Square::new(4, 2))));
    assert!(moves.contains(&Move::new(Square::new(4, 1), Square::new(4, 7))));
}

#[test]
fn queen_and_king_deliver_checkmate() {
    let mut board = Board::empty();
    place(&mut board, Square::new(0, 7), PieceKind::King, Color::Black);
    place(&mut board, Square::new(0, 6), PieceKind::Queen, Color::White);
    place(&mut board, Square::new(1, 5), PieceKind::King, Color::White);

    assert!(is_checkmate(&board, Color::Black));
}

#[test]
fn queen_and_king_produce_stalemate() {
    let mut board = Board::empty();
    place(&mut board, Square::new(7, 7), PieceKind::King, Color::Black);
    place(&mut board, Square::new(5, 6), PieceKind::King, Color::White);
    place(&mut board, Square::new(6, 5), PieceKind::Queen, Color::White);

    assert!(!is_in_check(&board, Color::Black));
    assert!(is_stalemate(&board, Color::Black));
}
