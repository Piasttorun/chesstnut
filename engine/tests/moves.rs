use chesstnut::engine::board::{PieceKind, Square};
use chesstnut::engine::moves::Move;

#[test]
fn plain_move_has_no_promotion() {
    let m = Move::new(Square::new(4, 1), Square::new(4, 3));
    assert_eq!(m.promotion, None);
    assert!(!m.is_promotion());
}

#[test]
fn promotion_move_carries_target_piece() {
    let m = Move::promotion(Square::new(0, 6), Square::new(0, 7), PieceKind::Queen);
    assert_eq!(m.promotion, Some(PieceKind::Queen));
    assert!(m.is_promotion());
}
