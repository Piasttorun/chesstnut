use crate::engine::board::{Board, Color, PieceKind, Square};

use super::Score;

fn piece_value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn => 100,
        PieceKind::Knight => 320,
        PieceKind::Bishop => 330,
        PieceKind::Rook => 500,
        PieceKind::Queen => 900,
        PieceKind::King => 0,
    }
}

/// Material-only evaluation, in centipawns, from White's perspective —
/// positive means White is ahead. No positional heuristics at all; this is
/// the simplest evaluation a chess engine can have, matching this stage's
/// "just show the current material balance" scope. Checkmate/stalemate
/// aren't special-cased here — spotting a forced mate needs the search
/// stage 3 will add, not a static board evaluation.
pub fn evaluate(board: &Board) -> Score {
    let mut total = 0;
    for index in 0..64 {
        if let Some(piece) = board.get(Square::from_index(index)) {
            let value = piece_value(piece.kind);
            total += match piece.color {
                Color::White => value,
                Color::Black => -value,
            };
        }
    }
    Score::Centipawns(total)
}
