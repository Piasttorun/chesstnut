use crate::engine::board::{Board, Color, Piece, PieceKind, Square};
use crate::engine::moves::Move;

/// Renders one move as Standard Algebraic Notation ("Nf3", "exd5", "O-O",
/// "e8=Q", "Qxe7+"). Takes the board *before* the move plus every legal move
/// available to the mover that turn (for disambiguation — "which knight
/// went to d2?") rather than a `Game`, same reasoning as fen.rs: this stays
/// a pure function of primitive engine types, and `Game::make_move` is the
/// one place with both the pre-move board and the after-the-fact check/mate
/// result needed to call it.
pub(crate) fn san(
    board_before: &Board,
    color: Color,
    legal_moves_before: &[Move],
    mv: Move,
    is_check: bool,
    is_checkmate: bool,
) -> String {
    let piece = board_before
        .get(mv.from)
        .expect("a move's `from` square must hold the piece that's moving");

    if piece.kind == PieceKind::King {
        let file_delta = mv.to.file as i8 - mv.from.file as i8;
        if file_delta == 2 {
            return with_suffix("O-O", is_check, is_checkmate);
        }
        if file_delta == -2 {
            return with_suffix("O-O-O", is_check, is_checkmate);
        }
    }

    let is_capture = board_before.get(mv.to).is_some()
        || (piece.kind == PieceKind::Pawn && mv.from.file != mv.to.file);

    let mut san = String::new();
    if piece.kind == PieceKind::Pawn {
        if is_capture {
            san.push(file_letter(mv.from.file));
            san.push('x');
        }
        san.push_str(&square_str(mv.to));
        if let Some(promotion) = mv.promotion {
            san.push('=');
            san.push(piece_letter(promotion));
        }
    } else {
        san.push(piece_letter(piece.kind));
        san.push_str(&disambiguation(board_before, color, legal_moves_before, piece.kind, mv));
        if is_capture {
            san.push('x');
        }
        san.push_str(&square_str(mv.to));
    }

    with_suffix(&san, is_check, is_checkmate)
}

/// Joins SAN strings into standard movetext: "1. e4 e5 2. Nf3 Nc6 ...".
pub(crate) fn movetext(moves: &[String]) -> String {
    let mut out = String::new();
    for (index, san) in moves.iter().enumerate() {
        if index % 2 == 0 {
            if index > 0 {
                out.push(' ');
            }
            out.push_str(&format!("{}. ", index / 2 + 1));
        } else {
            out.push(' ');
        }
        out.push_str(san);
    }
    out
}

/// Extracts the move tokens from PGN text — strips `[Tag "value"]` header
/// lines, `{comments}` and `(variations)` (including nested ones — a
/// variation can itself branch again), NAG codes (`$1`), move-number
/// prefixes ("1.", "23...", including when glued directly to the move like
/// "1.e4"), the trailing result token, and any "+"/"#"/"!"/"?" suffix
/// (annotation glyphs and check/mate suffixes carry no information a
/// legal-move lookup needs — see `apply_san` in game.rs, which matches
/// against SAN computed with is_check/is_checkmate forced to false). Real
/// exports (e.g. Lichess) routinely include comments/variations — someone
/// who took back a move during analysis gets the abandoned line preserved
/// as a `(...)` branch — so these are common, valid PGN, not malformed
/// input.
pub(crate) fn movetext_tokens(text: &str) -> Vec<String> {
    strip_annotations(&strip_headers(text))
        .split_whitespace()
        .filter_map(|raw| {
            let token = strip_move_number_prefix(raw);
            if token.is_empty() || is_result_token(token) || token.starts_with('$') {
                None
            } else {
                Some(token.trim_end_matches(['+', '#', '!', '?']).to_string())
            }
        })
        .collect()
}

fn strip_headers(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with('[') && trimmed.ends_with(']'))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Drops everything inside `{...}` or `(...)`, tracking each bracket kind's
/// nesting depth independently so a variation-within-a-variation is dropped
/// in full rather than leaving its tail exposed once the first `)` is hit.
fn strip_annotations(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut paren_depth: u32 = 0;
    let mut brace_depth: u32 = 0;

    for ch in text.chars() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            _ if paren_depth == 0 && brace_depth == 0 => out.push(ch),
            _ => {}
        }
    }

    out
}

fn strip_move_number_prefix(token: &str) -> &str {
    let after_digits = token.trim_start_matches(|ch: char| ch.is_ascii_digit());
    let digits_were_stripped = after_digits.len() < token.len();
    // Only a move number if a '.' actually follows the digits — otherwise
    // this is a leading digit that belongs to something else entirely, like
    // the "1" in the result token "1-0" or "1/2-1/2".
    if digits_were_stripped && after_digits.starts_with('.') {
        after_digits.trim_start_matches('.')
    } else {
        token
    }
}

fn is_result_token(token: &str) -> bool {
    matches!(token, "1-0" | "0-1" | "1/2-1/2" | "*")
}

fn with_suffix(san: &str, is_check: bool, is_checkmate: bool) -> String {
    if is_checkmate {
        format!("{san}#")
    } else if is_check {
        format!("{san}+")
    } else {
        san.to_string()
    }
}

/// SAN only adds enough to distinguish the mover from other same-kind,
/// same-color pieces that could *also* legally reach the same square: file
/// letter first, then rank if file alone isn't enough, then the full square
/// as a last resort (only needed with 3+ candidates, e.g. three queens).
fn disambiguation(board: &Board, color: Color, legal_moves: &[Move], kind: PieceKind, mv: Move) -> String {
    let rivals: Vec<Move> = legal_moves
        .iter()
        .copied()
        .filter(|other| {
            other.to == mv.to
                && other.from != mv.from
                && board.get(other.from) == Some(Piece { kind, color })
        })
        .collect();

    if rivals.is_empty() {
        return String::new();
    }

    let file_is_unique = rivals.iter().all(|rival| rival.from.file != mv.from.file);
    if file_is_unique {
        return file_letter(mv.from.file).to_string();
    }

    let rank_is_unique = rivals.iter().all(|rival| rival.from.rank != mv.from.rank);
    if rank_is_unique {
        return (mv.from.rank + 1).to_string();
    }

    square_str(mv.from)
}

fn piece_letter(kind: PieceKind) -> char {
    match kind {
        PieceKind::Knight => 'N',
        PieceKind::Bishop => 'B',
        PieceKind::Rook => 'R',
        PieceKind::Queen => 'Q',
        PieceKind::King => 'K',
        PieceKind::Pawn => unreachable!("pawn moves never carry a piece letter in SAN"),
    }
}

fn file_letter(file: u8) -> char {
    (b'a' + file) as char
}

fn square_str(square: Square) -> String {
    format!("{}{}", file_letter(square.file), square.rank + 1)
}
