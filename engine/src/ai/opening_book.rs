use rand::Rng;

use crate::engine::board::Square;
use crate::engine::game::Game;
use crate::engine::moves::Move;

/// A small, hand-picked set of well-known strong opening lines, as
/// sequences of moves in plain coordinate form ("e2e4" = e2 to e4). Not
/// remotely exhaustive opening theory — real opening books run to
/// millions of positions — just enough that the engine doesn't spend its
/// search budget "discovering" moves that have been known-good for
/// centuries, and doesn't play something objectively passive as an
/// opening move purely because material-only eval can't yet tell the
/// difference between that and fighting for the center.
const BOOK_LINES: &[&[&str]] = &[
    // Italian Game
    &["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "f8c5", "c2c3", "g8f6", "d2d3"],
    // Ruy Lopez
    &[
        "e2e4", "e7e5", "g1f3", "b8c6", "f1b5", "a7a6", "b5a4", "g8f6", "e1g1", "f8e7", "f1e1", "b7b5", "a4b3",
    ],
    // Scotch Game
    &["e2e4", "e7e5", "g1f3", "b8c6", "d2d4", "e5d4", "f3d4", "g8f6", "b1c3"],
    // Vienna Game
    &["e2e4", "e7e5", "b1c3", "g8f6", "f2f4"],
    // Scandinavian Defense
    &["e2e4", "d7d5", "e4d5", "d8d5", "b1c3", "d5a5", "d2d4", "g8f6"],
    // Open Sicilian (Najdorf-ish)
    &[
        "e2e4", "c7c5", "g1f3", "d7d6", "d2d4", "c5d4", "f3d4", "g8f6", "b1c3", "a7a6",
    ],
    // Caro-Kann
    &["e2e4", "c7c6", "d2d4", "d7d5", "b1c3", "d5e4", "c3e4", "c8f5", "e4g3", "f5g6"],
    // French Defense
    &[
        "e2e4", "e7e6", "d2d4", "d7d5", "b1c3", "g8f6", "c1g5", "f8e7", "e4e5", "f6d7",
    ],
    // Pirc / Modern Defense
    &["e2e4", "d7d6", "d2d4", "g8f6", "b1c3", "g7g6", "g1f3", "f8g7"],
    // Queen's Gambit (Declined)
    &[
        "d2d4", "d7d5", "c2c4", "e7e6", "b1c3", "g8f6", "c1g5", "f8e7", "e2e3", "e8g8",
    ],
    // Slav Defense
    &["d2d4", "d7d5", "c2c4", "c7c6", "g1f3", "g8f6", "b1c3", "d5c4"],
    // Nimzo-Indian
    &["d2d4", "g8f6", "c2c4", "e7e6", "b1c3", "f8b4", "e2e3", "e8g8"],
    // King's Indian setup
    &[
        "d2d4", "g8f6", "c2c4", "g7g6", "b1c3", "f8g7", "e2e4", "d7d6", "g1f3", "e8g8",
    ],
    // London System
    &[
        "d2d4", "d7d5", "c1f4", "g8f6", "e2e3", "e7e6", "g1f3", "f8d6", "f4g3", "e8g8", "f1d3",
    ],
    // London System (vs. an early ...c5)
    &["d2d4", "g8f6", "c1f4", "c7c5", "e2e3", "d8b6", "b1c3"],
    // Dutch Defense
    &["d2d4", "f7f5", "g2g3", "g8f6", "f1g2", "e7e6", "g1f3", "f8e7"],
    // Reti / English transposition
    &["g1f3", "d7d5", "c2c4", "e7e6", "b2b3"],
    // English Opening
    &["c2c4", "e7e5", "b1c3", "g8f6", "g1f3", "b8c6"],
];

fn parse_square(text: &str) -> Option<Square> {
    let bytes = text.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    let file = bytes[0].checked_sub(b'a')?;
    let rank = bytes[1].checked_sub(b'1')?;
    (file <= 7 && rank <= 7).then(|| Square::new(file, rank))
}

fn parse_coord_move(text: &str) -> Option<(Square, Square)> {
    if text.len() != 4 {
        return None;
    }
    Some((parse_square(&text[0..2])?, parse_square(&text[2..4])?))
}

/// If `game`'s move history so far is an exact prefix of one or more of
/// [`BOOK_LINES`], returns a move one of those lines continues with —
/// chosen uniformly at random among every *distinct* move any matching
/// line proposes, so the engine doesn't play the exact same opening every
/// single game purely because, say, the Italian Game happens to be listed
/// before the Ruy Lopez. Returns `None` once the game has left every known
/// line (real search takes over from there) or if nothing in the book
/// matches at all (an opponent playing something unusual, say).
///
/// Matches by replaying each candidate line from a fresh standard-starting-
/// position game and comparing the resulting FEN against `game`'s actual
/// FEN, rather than comparing move counts/notation — the book only ever
/// applies to an actual game played out from the normal starting position;
/// comparing move *history length* alone isn't enough; an arbitrary
/// position (a hand-built puzzle, say) can just as easily report zero
/// moves played, and would otherwise look like a trivial match against
/// every line's empty prefix.
pub fn book_move(game: &Game) -> Option<Move> {
    if game.awaiting_clock_choice() {
        // Matches search::think's own guard — nothing to play before a
        // time control has even been chosen.
        return None;
    }

    let played = game.move_history();
    let mut candidates: Vec<Move> = Vec::new();

    'lines: for line in BOOK_LINES {
        if played.len() >= line.len() {
            continue; // this line has already been fully played out
        }

        let mut replay = Game::new();
        for coord in &line[..played.len()] {
            let (from, to) = parse_coord_move(coord)?;
            let mv = replay.legal_moves_from(from).into_iter().find(|mv| mv.to == to)?;
            if replay.make_move(mv).is_err() {
                continue 'lines;
            }
        }

        if replay.to_fen() != game.to_fen() {
            continue;
        }

        let (from, to) = parse_coord_move(line[played.len()])?;
        if let Some(mv) = replay.legal_moves_from(from).into_iter().find(|mv| mv.to == to) {
            // Two lines can share a prefix and still propose the same next
            // move (e.g. two different Ruy Lopez branches both playing
            // 3...a6) — deduplicated so that move isn't more likely to be
            // picked just because more lines happen to agree on it.
            if !candidates.contains(&mv) {
                candidates.push(mv);
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }
    let index = rand::thread_rng().gen_range(0..candidates.len());
    Some(candidates[index])
}
