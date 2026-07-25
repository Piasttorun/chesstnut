use crate::engine::board::{Board, Color, PieceKind, Square};
use crate::engine::moves::Move;
use crate::engine::pieces::candidate_moves;

fn find_king(board: &Board, color: Color) -> Option<Square> {
    for index in 0..64 {
        let square = Square::from_index(index);
        if let Some(piece) = board.get(square) {
            if piece.kind == PieceKind::King && piece.color == color {
                return Some(square);
            }
        }
    }
    None
}

pub(crate) fn is_square_attacked(board: &Board, square: Square, by_color: Color) -> bool {
    for index in 0..64 {
        let from = Square::from_index(index);
        if let Some(piece) = board.get(from) {
            if piece.color == by_color && candidate_moves(board, from).contains(&square) {
                return true;
            }
        }
    }
    false
}

pub fn is_in_check(board: &Board, color: Color) -> bool {
    match find_king(board, color) {
        Some(king_square) => is_square_attacked(board, king_square, color.opponent()),
        None => false,
    }
}

/// Mechanically applies a move, including the side effects of the two moves
/// that aren't fully described by a plain (from, to): castling and en
/// passant. Neither needs a flag on `Move` — both are unambiguous from shape
/// alone: a king moving two files is always a castle, and a pawn moving
/// diagonally onto an *empty* square is always an en passant capture (a
/// normal diagonal pawn move is only ever a capture of whatever sits on the
/// destination square).
pub(crate) fn apply_move(board: &Board, mv: Move) -> Board {
    let mut next = *board;
    if let Some(mut piece) = next.get(mv.from) {
        if piece.kind == PieceKind::Pawn && mv.from.file != mv.to.file && next.get(mv.to).is_none() {
            // En passant: the captured pawn sits on the mover's start rank,
            // under the destination square — not on the destination itself.
            next.set(Square::new(mv.to.file, mv.from.rank), None);
        }

        if piece.kind == PieceKind::King {
            let file_delta = mv.to.file as i8 - mv.from.file as i8;
            if file_delta == 2 {
                move_rook(&mut next, Square::new(7, mv.from.rank), Square::new(5, mv.from.rank));
            } else if file_delta == -2 {
                move_rook(&mut next, Square::new(0, mv.from.rank), Square::new(3, mv.from.rank));
            }
        }

        if let Some(promotion) = mv.promotion {
            piece.kind = promotion;
        }
        next.set(mv.from, None);
        next.set(mv.to, Some(piece));
    }
    next
}

fn move_rook(board: &mut Board, from: Square, to: Square) {
    if let Some(rook) = board.get(from) {
        board.set(from, None);
        board.set(to, Some(rook));
    }
}

fn expand_promotions(kind: PieceKind, from: Square, to: Square) -> Vec<Move> {
    let is_promotion_rank = to.rank == 0 || to.rank == 7;
    if kind != PieceKind::Pawn || !is_promotion_rank {
        return vec![Move::new(from, to)];
    }

    vec![
        Move::promotion(from, to, PieceKind::Queen),
        Move::promotion(from, to, PieceKind::Rook),
        Move::promotion(from, to, PieceKind::Bishop),
        Move::promotion(from, to, PieceKind::Knight),
    ]
}

/// All fully legal moves for the piece on `from`: pseudo-legal candidates
/// from `pieces::candidate_moves`, minus any that would leave the mover's
/// own king in check, with pawn promotions expanded into one Move per
/// promotion piece.
pub fn legal_moves_from(board: &Board, from: Square) -> Vec<Move> {
    let piece = match board.get(from) {
        Some(piece) => piece,
        None => return Vec::new(),
    };

    let mover_color = piece.color;
    let mut legal = Vec::new();

    for to in candidate_moves(board, from) {
        for mv in expand_promotions(piece.kind, from, to) {
            let hypothetical = apply_move(board, mv);
            if !is_in_check(&hypothetical, mover_color) {
                legal.push(mv);
            }
        }
    }

    legal
}

pub fn legal_moves_for(board: &Board, color: Color) -> Vec<Move> {
    let mut all_moves = Vec::new();
    for index in 0..64 {
        let square = Square::from_index(index);
        if let Some(piece) = board.get(square) {
            if piece.color == color {
                all_moves.extend(legal_moves_from(board, square));
            }
        }
    }
    all_moves
}

pub fn is_checkmate(board: &Board, color: Color) -> bool {
    is_in_check(board, color) && legal_moves_for(board, color).is_empty()
}

pub fn is_stalemate(board: &Board, color: Color) -> bool {
    !is_in_check(board, color) && legal_moves_for(board, color).is_empty()
}

