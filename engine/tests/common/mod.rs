use chesstnut::engine::board::{Board, Color, Piece, PieceKind, Square};
use chesstnut::engine::game::Game;
use chesstnut::engine::moves::Move;

pub fn place(board: &mut Board, square: Square, kind: PieceKind, color: Color) {
    board.set(square, Some(Piece { kind, color }));
}

// Two closed king-walks — each square visited exactly once per lap, unlike
// a back-and-forth bounce — used to run many non-repeating half-moves for
// fifty-move-rule tests. A bounce always revisits its own interior squares
// twice per lap (once on the way out, once on the way back), and that
// doubling can line up with the *other* king's cycle and trip threefold
// repetition far sooner than expected — a true loop has no such internal
// repeat. These two loop lengths (11 and 8) are coprime, so the combined
// two-king position only repeats every 11*8 = 88 rounds, well past
// anything these tests need.
const WHITE_LOOP: [Square; 11] = [
    Square { file: 0, rank: 4 },
    Square { file: 1, rank: 4 },
    Square { file: 2, rank: 4 },
    Square { file: 3, rank: 4 },
    Square { file: 4, rank: 4 },
    Square { file: 5, rank: 4 },
    Square { file: 4, rank: 5 },
    Square { file: 3, rank: 5 },
    Square { file: 2, rank: 5 },
    Square { file: 1, rank: 5 },
    Square { file: 0, rank: 5 },
];

const BLACK_LOOP: [Square; 8] = [
    Square { file: 0, rank: 0 },
    Square { file: 1, rank: 0 },
    Square { file: 2, rank: 0 },
    Square { file: 3, rank: 0 },
    Square { file: 3, rank: 1 },
    Square { file: 2, rank: 1 },
    Square { file: 1, rank: 1 },
    Square { file: 0, rank: 1 },
];

pub fn white_shuffle_start() -> Square {
    WHITE_LOOP[0]
}

pub fn black_shuffle_start() -> Square {
    BLACK_LOOP[0]
}

pub fn white_step_square(step: usize) -> Square {
    WHITE_LOOP[step % WHITE_LOOP.len()]
}

pub fn black_step_square(step: usize) -> Square {
    BLACK_LOOP[step % BLACK_LOOP.len()]
}

/// Advances one round (white then black) along the fixed loops above.
/// `white_step`/`black_step` are separate counters so one side can sit out a
/// round (e.g. to splice in a capture) without desyncing the other.
pub fn shuffle_round(game: &mut Game, white_step: &mut usize, black_step: &mut usize) {
    let white_from = white_step_square(*white_step);
    *white_step += 1;
    let white_to = white_step_square(*white_step);
    game.make_move(Move::new(white_from, white_to)).unwrap();

    let black_from = black_step_square(*black_step);
    *black_step += 1;
    let black_to = black_step_square(*black_step);
    game.make_move(Move::new(black_from, black_to)).unwrap();
}
