use crate::engine::board::{Board, Color, PieceKind, Square};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SquareStatus {
    Empty,
    Friendly,
    Enemy,
}

fn square_status(board: &Board, square: Square, color: Color) -> SquareStatus {
    match board.get(square) {
        None => SquareStatus::Empty,
        Some(piece) if piece.color == color => SquareStatus::Friendly,
        Some(_) => SquareStatus::Enemy,
    }
}

fn offset(square: Square, file_delta: i8, rank_delta: i8) -> Option<Square> {
    let file = square.file as i8 + file_delta;
    let rank = square.rank as i8 + rank_delta;
    if file < 0 || file > 7 || rank < 0 || rank > 7 {
        None
    } else {
        Some(Square::new(file as u8, rank as u8))
    }
}

const KNIGHT_OFFSETS: [(i8, i8); 8] = [
    (1, 2),
    (2, 1),
    (2, -1),
    (1, -2),
    (-1, -2),
    (-2, -1),
    (-2, 1),
    (-1, 2),
];

const KING_OFFSETS: [(i8, i8); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

const ROOK_DIRECTIONS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const BISHOP_DIRECTIONS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
const QUEEN_DIRECTIONS: [(i8, i8); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// One-step movers: knight and king each check a fixed set of offsets,
/// no matter what else is on the board.
fn stepper_moves(board: &Board, from: Square, color: Color, offsets: &[(i8, i8)]) -> Vec<Square> {
    let mut moves = Vec::new();
    for &(file_delta, rank_delta) in offsets {
        if let Some(to) = offset(from, file_delta, rank_delta) {
            match square_status(board, to, color) {
                SquareStatus::Friendly => {}
                SquareStatus::Empty | SquareStatus::Enemy => moves.push(to),
            }
        }
    }
    moves
}

/// Sliding movers: bishop, rook, queen each walk a direction until they
/// fall off the board, hit a friendly piece (stop before it), or hit an
/// enemy piece (include that square, then stop).
fn sliding_moves(board: &Board, from: Square, color: Color, directions: &[(i8, i8)]) -> Vec<Square> {
    let mut moves = Vec::new();
    for &(file_delta, rank_delta) in directions {
        let mut current = from;
        while let Some(to) = offset(current, file_delta, rank_delta) {
            match square_status(board, to, color) {
                SquareStatus::Empty => {
                    moves.push(to);
                    current = to;
                }
                SquareStatus::Enemy => {
                    moves.push(to);
                    break;
                }
                SquareStatus::Friendly => break,
            }
        }
    }
    moves
}

fn pawn_moves(board: &Board, from: Square, color: Color) -> Vec<Square> {
    let mut moves = Vec::new();
    let (direction, start_rank) = match color {
        Color::White => (1, 1),
        Color::Black => (-1, 6),
    };

    if let Some(one_step) = offset(from, 0, direction) {
        if square_status(board, one_step, color) == SquareStatus::Empty {
            moves.push(one_step);

            if from.rank == start_rank {
                if let Some(two_step) = offset(from, 0, direction * 2) {
                    if square_status(board, two_step, color) == SquareStatus::Empty {
                        moves.push(two_step);
                    }
                }
            }
        }
    }

    for file_delta in [-1, 1] {
        if let Some(capture_square) = offset(from, file_delta, direction) {
            if square_status(board, capture_square, color) == SquareStatus::Enemy {
                moves.push(capture_square);
            }
        }
    }

    moves
}

/// Pseudo-legal destination squares for the piece on `from`: obeys how each
/// piece moves and what's blocking it, but does NOT check whether making the
/// move would leave the mover's own king in check. That check-safety filter
/// is `rules.rs`'s job, applied on top of these candidates.
pub fn candidate_moves(board: &Board, from: Square) -> Vec<Square> {
    match board.get(from) {
        Some(piece) => match piece.kind {
            PieceKind::Knight => stepper_moves(board, from, piece.color, &KNIGHT_OFFSETS),
            PieceKind::King => stepper_moves(board, from, piece.color, &KING_OFFSETS),
            PieceKind::Bishop => sliding_moves(board, from, piece.color, &BISHOP_DIRECTIONS),
            PieceKind::Rook => sliding_moves(board, from, piece.color, &ROOK_DIRECTIONS),
            PieceKind::Queen => sliding_moves(board, from, piece.color, &QUEEN_DIRECTIONS),
            PieceKind::Pawn => pawn_moves(board, from, piece.color),
        },
        None => Vec::new(),
    }
}

