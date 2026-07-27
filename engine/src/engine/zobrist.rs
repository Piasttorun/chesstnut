// Zobrist hashing: a 64-bit fingerprint of a position, built by XORing
// together one random-looking number per (piece, square) currently
// occupied, plus numbers for whose turn it is, which castling rights still
// stand, and the en passant file if any. XOR is its own inverse, which is
// the whole appeal for a *real* engine doing incremental updates (undo a
// move by XORing the same numbers back in) — this first version doesn't
// do that yet (see `hash` below), but the key still needs to be built this
// way so two boards that are actually the same position always produce the
// same hash regardless of what order their pieces happen to be visited in.
//
// Used by the AI's transposition table (see `ai::search`) to recognize a
// position it's already searched, even if the moves that reached it came
// in a different order the first time.
use std::sync::OnceLock;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use super::board::{Board, Color, PieceKind, Square};

struct ZobristKeys {
    // Indexed by [piece_index(kind, color)][square index 0..64].
    piece_square: [[u64; 64]; 12],
    side_to_move: u64,
    // white_kingside, white_queenside, black_kingside, black_queenside, in
    // that order — matches CastlingRights' field order in game.rs.
    castling: [u64; 4],
    // Only the file matters for en passant's effect on legal moves, not
    // the full square.
    en_passant_file: [u64; 8],
}

fn piece_index(kind: PieceKind, color: Color) -> usize {
    let kind_index = match kind {
        PieceKind::Pawn => 0,
        PieceKind::Knight => 1,
        PieceKind::Bishop => 2,
        PieceKind::Rook => 3,
        PieceKind::Queen => 4,
        PieceKind::King => 5,
    };
    kind_index + if color == Color::Black { 6 } else { 0 }
}

fn keys() -> &'static ZobristKeys {
    static KEYS: OnceLock<ZobristKeys> = OnceLock::new();
    KEYS.get_or_init(|| {
        // A fixed seed rather than real entropy — this hash only needs to
        // be internally consistent within one run of the app (it's a
        // transposition-table key, not a security value), and a fixed seed
        // means a hash mismatch is reproducible from run to run instead of
        // depending on process-specific randomness.
        let mut rng = StdRng::seed_from_u64(0xC435_57A1_7095_71A5);
        let mut piece_square = [[0u64; 64]; 12];
        for table in piece_square.iter_mut() {
            for slot in table.iter_mut() {
                *slot = rng.gen();
            }
        }
        let side_to_move = rng.gen();
        let mut castling = [0u64; 4];
        for slot in castling.iter_mut() {
            *slot = rng.gen();
        }
        let mut en_passant_file = [0u64; 8];
        for slot in en_passant_file.iter_mut() {
            *slot = rng.gen();
        }
        ZobristKeys { piece_square, side_to_move, castling, en_passant_file }
    })
}

/// Everything that affects which moves are legal from here — matching this
/// exactly is what makes the hash safe to use as a transposition-table key.
/// Leaving out, say, castling rights would let the table equate two
/// positions that look identical on the board but actually allow different
/// moves, which is a real correctness bug (a "false transposition"), not
/// just a missed cache hit.
pub(crate) fn hash(
    board: &Board,
    turn: Color,
    castling_rights: [bool; 4],
    en_passant_target: Option<Square>,
) -> u64 {
    let keys = keys();
    let mut result = 0u64;

    for index in 0..64 {
        if let Some(piece) = board.get(Square::from_index(index)) {
            result ^= keys.piece_square[piece_index(piece.kind, piece.color)][index];
        }
    }
    if turn == Color::Black {
        result ^= keys.side_to_move;
    }
    for (&flag, &key) in castling_rights.iter().zip(keys.castling.iter()) {
        if flag {
            result ^= key;
        }
    }
    if let Some(square) = en_passant_target {
        result ^= keys.en_passant_file[square.file as usize];
    }

    result
}
