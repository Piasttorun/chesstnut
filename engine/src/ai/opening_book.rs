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
    &["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "f8c5"],
    // Ruy Lopez
    &["e2e4", "e7e5", "g1f3", "b8c6", "f1b5", "a7a6", "b5a4", "g8f6"],
    // Queen's Gambit
    &["d2d4", "d7d5", "c2c4", "e7e6", "b1c3", "g8f6"],
    // Open Sicilian
    &["e2e4", "c7c5", "g1f3", "d7d6", "d2d4", "c5d4", "f3d4", "g8f6"],
    // Caro-Kann
    &["e2e4", "c7c6", "d2d4", "d7d5", "b1c3", "d5e4", "c3e4"],
    // French Defense
    &["e2e4", "e7e6", "d2d4", "d7d5", "b1c3", "g8f6"],
    // King's Indian setup
    &["d2d4", "g8f6", "c2c4", "g7g6", "b1c3", "f8g7"],
    // English Opening
    &["c2c4", "e7e5", "b1c3", "g8f6"],
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

/// If `game`'s move history so far is an exact prefix of one of
/// [`BOOK_LINES`], returns the move that line continues with — `None` once
/// the game has left known book territory (which, for a handful of short
/// lines, happens quickly; real search takes over from there) or if
/// nothing in the book matches at all (an opponent playing something
/// unusual, say).
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
        return replay.legal_moves_from(from).into_iter().find(|mv| mv.to == to);
    }

    None
}
