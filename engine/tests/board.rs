use chesstnut::engine::board::{Board, Color, Piece, PieceKind, Square};

#[test]
fn square_index_round_trips() {
    let square = Square::new(3, 5);
    assert_eq!(Square::from_index(square.to_index()), square);
}

#[test]
fn empty_board_has_no_pieces() {
    let board = Board::empty();
    assert_eq!(board.get(Square::new(0, 0)), None);
    assert_eq!(board.get(Square::new(7, 7)), None);
}

#[test]
fn starting_position_places_white_king_on_e1() {
    let board = Board::starting_position();
    let king = board.get(Square::new(4, 0));
    assert_eq!(
        king,
        Some(Piece {
            kind: PieceKind::King,
            color: Color::White,
        })
    );
}

#[test]
fn starting_position_has_32_pieces() {
    let board = Board::starting_position();
    let count = (0..64)
        .filter(|&i| board.get(Square::from_index(i)).is_some())
        .count();
    assert_eq!(count, 32);
}
