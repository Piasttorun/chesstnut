use crate::engine::board::{Board, Color, PieceKind, Square};

use super::Score;

// pub(crate) rather than private so search.rs (a sibling module under `ai`)
// can reuse it for move ordering — ranking captures by the value of what
// they take.
pub(crate) fn piece_value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn => 100,
        PieceKind::Knight => 320,
        PieceKind::Bishop => 330,
        PieceKind::Rook => 500,
        PieceKind::Queen => 900,
        PieceKind::King => 0,
    }
}

// Classic "simplified evaluation function" piece-square tables (Tomasz
// Michniewski). Written exactly as they're always published: row 0 is
// rank 8, row 7 is rank 1, from White's point of view. No separate
// endgame king table — that's a known simplification, not an oversight;
// it'd need game-phase detection to blend between tables sensibly.
#[rustfmt::skip]
const PAWN_TABLE: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
    50, 50, 50, 50, 50, 50, 50, 50,
    10, 10, 20, 30, 30, 20, 10, 10,
     5,  5, 10, 25, 25, 10,  5,  5,
     0,  0,  0, 20, 20,  0,  0,  0,
     5, -5,-10,  0,  0,-10, -5,  5,
     5, 10, 10,-20,-20, 10, 10,  5,
     0,  0,  0,  0,  0,  0,  0,  0,
];

#[rustfmt::skip]
const KNIGHT_TABLE: [i32; 64] = [
    -50,-40,-30,-30,-30,-30,-40,-50,
    -40,-20,  0,  0,  0,  0,-20,-40,
    -30,  0, 10, 15, 15, 10,  0,-30,
    -30,  5, 15, 20, 20, 15,  5,-30,
    -30,  0, 15, 20, 20, 15,  0,-30,
    -30,  5, 10, 15, 15, 10,  5,-30,
    -40,-20,  0,  5,  5,  0,-20,-40,
    -50,-40,-30,-30,-30,-30,-40,-50,
];

#[rustfmt::skip]
const BISHOP_TABLE: [i32; 64] = [
    -20,-10,-10,-10,-10,-10,-10,-20,
    -10,  0,  0,  0,  0,  0,  0,-10,
    -10,  0,  5, 10, 10,  5,  0,-10,
    -10,  5,  5, 10, 10,  5,  5,-10,
    -10,  0, 10, 10, 10, 10,  0,-10,
    -10, 10, 10, 10, 10, 10, 10,-10,
    -10,  5,  0,  0,  0,  0,  5,-10,
    -20,-10,-10,-10,-10,-10,-10,-20,
];

#[rustfmt::skip]
const ROOK_TABLE: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
     5, 10, 10, 10, 10, 10, 10,  5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
     0,  0,  0,  5,  5,  0,  0,  0,
];

#[rustfmt::skip]
const QUEEN_TABLE: [i32; 64] = [
    -20,-10,-10, -5, -5,-10,-10,-20,
    -10,  0,  0,  0,  0,  0,  0,-10,
    -10,  0,  5,  5,  5,  5,  0,-10,
     -5,  0,  5,  5,  5,  5,  0, -5,
      0,  0,  5,  5,  5,  5,  0, -5,
    -10,  5,  5,  5,  5,  5,  0,-10,
    -10,  0,  5,  0,  0,  0,  0,-10,
    -20,-10,-10, -5, -5,-10,-10,-20,
];

#[rustfmt::skip]
const KING_TABLE: [i32; 64] = [
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -20,-30,-30,-40,-40,-30,-30,-20,
    -10,-20,-20,-20,-20,-20,-20,-10,
     20, 20,  0,  0,  0,  0, 20, 20,
     20, 30, 10,  0,  0, 10, 30, 20,
];

fn table_for(kind: PieceKind) -> &'static [i32; 64] {
    match kind {
        PieceKind::Pawn => &PAWN_TABLE,
        PieceKind::Knight => &KNIGHT_TABLE,
        PieceKind::Bishop => &BISHOP_TABLE,
        PieceKind::Rook => &ROOK_TABLE,
        PieceKind::Queen => &QUEEN_TABLE,
        PieceKind::King => &KING_TABLE,
    }
}

// The tables above are written rank-8-first (as always published), but our
// Square has rank 0 = rank 1. White reads the table top-down from its own
// back rank, so its row is `7 - rank`. Black's pieces advance the opposite
// direction, so mirroring the board vertically for Black is equivalent to
// reading the same table with the row un-flipped (`rank` as-is) — no
// separate table needed since every table here is left-right symmetric.
fn piece_square_value(kind: PieceKind, color: Color, square: Square) -> i32 {
    let row = match color {
        Color::White => 7 - square.rank as usize,
        Color::Black => square.rank as usize,
    };
    table_for(kind)[row * 8 + square.file as usize]
}

/// Material plus piece-square positional bonuses, in centipawns, from
/// White's perspective — positive means White is ahead. Checkmate/
/// stalemate aren't special-cased here — spotting a forced mate needs the
/// search stage, not a static board evaluation.
pub fn evaluate(board: &Board) -> Score {
    let mut total = 0;
    for index in 0..64 {
        if let Some(piece) = board.get(Square::from_index(index)) {
            let square = Square::from_index(index);
            let value = piece_value(piece.kind) + piece_square_value(piece.kind, piece.color, square);
            total += match piece.color {
                Color::White => value,
                Color::Black => -value,
            };
        }
    }
    Score::Centipawns(total)
}
