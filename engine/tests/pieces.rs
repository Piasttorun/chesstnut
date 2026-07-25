use chesstnut::engine::board::{Board, Color, Piece, PieceKind, Square};
use chesstnut::engine::pieces::candidate_moves;

fn place(board: &mut Board, square: Square, kind: PieceKind, color: Color) {
    board.set(square, Some(Piece { kind, color }));
}

#[test]
fn knight_in_the_open_has_eight_moves() {
    let mut board = Board::empty();
    let from = Square::new(4, 4);
    place(&mut board, from, PieceKind::Knight, Color::White);

    assert_eq!(candidate_moves(&board, from).len(), 8);
}

#[test]
fn knight_in_the_corner_has_two_moves() {
    let mut board = Board::empty();
    let from = Square::new(0, 0);
    place(&mut board, from, PieceKind::Knight, Color::White);

    assert_eq!(candidate_moves(&board, from).len(), 2);
}

#[test]
fn rook_slides_until_blocked_by_own_piece() {
    let mut board = Board::empty();
    let from = Square::new(0, 0);
    place(&mut board, from, PieceKind::Rook, Color::White);
    place(&mut board, Square::new(0, 3), PieceKind::Pawn, Color::White);

    let moves = candidate_moves(&board, from);
    assert!(moves.contains(&Square::new(0, 1)));
    assert!(moves.contains(&Square::new(0, 2)));
    assert!(!moves.contains(&Square::new(0, 3)));
    assert!(!moves.contains(&Square::new(0, 4)));
}

#[test]
fn rook_can_capture_an_enemy_but_not_jump_past_it() {
    let mut board = Board::empty();
    let from = Square::new(0, 0);
    place(&mut board, from, PieceKind::Rook, Color::White);
    place(&mut board, Square::new(0, 3), PieceKind::Pawn, Color::Black);

    let moves = candidate_moves(&board, from);
    assert!(moves.contains(&Square::new(0, 3)));
    assert!(!moves.contains(&Square::new(0, 4)));
}

#[test]
fn pawn_on_starting_rank_can_advance_two_squares() {
    let mut board = Board::empty();
    let from = Square::new(4, 1);
    place(&mut board, from, PieceKind::Pawn, Color::White);

    let moves = candidate_moves(&board, from);
    assert!(moves.contains(&Square::new(4, 2)));
    assert!(moves.contains(&Square::new(4, 3)));
}

#[test]
fn pawn_off_starting_rank_advances_one_square_only() {
    let mut board = Board::empty();
    let from = Square::new(4, 2);
    place(&mut board, from, PieceKind::Pawn, Color::White);

    let moves = candidate_moves(&board, from);
    assert!(moves.contains(&Square::new(4, 3)));
    assert!(!moves.contains(&Square::new(4, 4)));
}

#[test]
fn pawn_captures_diagonally_only_when_enemy_present() {
    let mut board = Board::empty();
    let from = Square::new(4, 4);
    place(&mut board, from, PieceKind::Pawn, Color::White);
    place(&mut board, Square::new(5, 5), PieceKind::Pawn, Color::Black);

    let moves = candidate_moves(&board, from);
    assert!(moves.contains(&Square::new(5, 5)));
    assert!(!moves.contains(&Square::new(3, 5)));
}

#[test]
fn empty_square_has_no_candidate_moves() {
    let board = Board::empty();
    assert!(candidate_moves(&board, Square::new(4, 4)).is_empty());
}
