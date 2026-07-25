use crate::engine::board::{piece_symbol, Board, Color, Piece, PieceKind, Square};

pub(crate) struct ParsedFen {
    pub board: Board,
    pub turn: Color,
    pub white_kingside: bool,
    pub white_queenside: bool,
    pub black_kingside: bool,
    pub black_queenside: bool,
    pub en_passant_target: Option<Square>,
    pub halfmove_clock: u32,
    pub fullmove_number: u32,
}

/// Formats a position as FEN — the standard `<placement> <turn> <castling>
/// <en passant> <halfmove> <fullmove>` string. Takes plain values rather
/// than a `Game` so this stays a pure function of board.rs/moves.rs types,
/// same as rules.rs and pieces.rs — `Game` is the one place that knows how
/// to gather its own private fields into this call.
pub(crate) fn format(
    board: &Board,
    turn: Color,
    white_kingside: bool,
    white_queenside: bool,
    black_kingside: bool,
    black_queenside: bool,
    en_passant_target: Option<Square>,
    halfmove_clock: u32,
    fullmove_number: u32,
) -> String {
    let placement = placement_field(board);
    let active_color = match turn {
        Color::White => "w",
        Color::Black => "b",
    };
    let castling = castling_field(white_kingside, white_queenside, black_kingside, black_queenside);
    let en_passant = match en_passant_target {
        Some(square) => square_str(square),
        None => "-".to_string(),
    };

    format!("{placement} {active_color} {castling} {en_passant} {halfmove_clock} {fullmove_number}")
}

fn placement_field(board: &Board) -> String {
    let mut ranks = Vec::new();

    for rank in (0..8).rev() {
        let mut row = String::new();
        let mut empty_run = 0;

        for file in 0..8 {
            match board.get(Square::new(file, rank)) {
                None => empty_run += 1,
                Some(piece) => {
                    if empty_run > 0 {
                        row.push_str(&empty_run.to_string());
                        empty_run = 0;
                    }
                    row.push(piece_symbol(piece));
                }
            }
        }

        if empty_run > 0 {
            row.push_str(&empty_run.to_string());
        }

        ranks.push(row);
    }

    ranks.join("/")
}

fn castling_field(white_kingside: bool, white_queenside: bool, black_kingside: bool, black_queenside: bool) -> String {
    let mut field = String::new();
    if white_kingside {
        field.push('K');
    }
    if white_queenside {
        field.push('Q');
    }
    if black_kingside {
        field.push('k');
    }
    if black_queenside {
        field.push('q');
    }

    if field.is_empty() {
        "-".to_string()
    } else {
        field
    }
}

fn square_str(square: Square) -> String {
    format!("{}{}", (b'a' + square.file) as char, square.rank + 1)
}

/// Parses a FEN string's six space-separated fields. Only validates syntax
/// (right shape, valid characters) — not chess legality (e.g. this won't
/// reject a position with three kings). Malformed input just fails with an
/// error; there's no attempt to recover or guess intent.
pub(crate) fn parse(text: &str) -> Result<ParsedFen, String> {
    let fields: Vec<&str> = text.split_whitespace().collect();
    let [placement, turn, castling, en_passant, halfmove, fullmove] = fields.as_slice() else {
        return Err(format!(
            "expected 6 space-separated fields, found {}",
            fields.len()
        ));
    };

    let board = parse_placement(placement)?;
    let turn = match *turn {
        "w" => Color::White,
        "b" => Color::Black,
        other => return Err(format!("invalid active color: '{other}'")),
    };
    let (white_kingside, white_queenside, black_kingside, black_queenside) = parse_castling(castling)?;
    let en_passant_target = parse_en_passant(en_passant)?;
    let halfmove_clock = halfmove
        .parse::<u32>()
        .map_err(|_| format!("invalid halfmove clock: '{halfmove}'"))?;
    let fullmove_number = fullmove
        .parse::<u32>()
        .map_err(|_| format!("invalid fullmove number: '{fullmove}'"))?;

    Ok(ParsedFen {
        board,
        turn,
        white_kingside,
        white_queenside,
        black_kingside,
        black_queenside,
        en_passant_target,
        halfmove_clock,
        fullmove_number,
    })
}

fn parse_placement(field: &str) -> Result<Board, String> {
    let ranks: Vec<&str> = field.split('/').collect();
    if ranks.len() != 8 {
        return Err(format!(
            "expected 8 ranks separated by '/', found {}",
            ranks.len()
        ));
    }

    let mut board = Board::empty();

    for (index, rank_str) in ranks.iter().enumerate() {
        // FEN lists ranks top-down (rank 8 first), so the i-th field is
        // rank index (7 - i).
        let rank = 7 - index as u8;
        let mut file: u8 = 0;

        for ch in rank_str.chars() {
            if let Some(empty_count) = ch.to_digit(10) {
                file += empty_count as u8;
            } else {
                if file >= 8 {
                    return Err(format!("rank '{rank_str}' has too many squares"));
                }
                board.set(Square::new(file, rank), Some(parse_piece_char(ch)?));
                file += 1;
            }
        }

        if file != 8 {
            return Err(format!("rank '{rank_str}' does not add up to 8 squares"));
        }
    }

    Ok(board)
}

fn parse_piece_char(ch: char) -> Result<Piece, String> {
    let color = if ch.is_ascii_uppercase() {
        Color::White
    } else {
        Color::Black
    };
    let kind = match ch.to_ascii_lowercase() {
        'p' => PieceKind::Pawn,
        'n' => PieceKind::Knight,
        'b' => PieceKind::Bishop,
        'r' => PieceKind::Rook,
        'q' => PieceKind::Queen,
        'k' => PieceKind::King,
        other => return Err(format!("invalid piece character: '{other}'")),
    };
    Ok(Piece { kind, color })
}

fn parse_castling(field: &str) -> Result<(bool, bool, bool, bool), String> {
    if field == "-" {
        return Ok((false, false, false, false));
    }

    let (mut white_kingside, mut white_queenside, mut black_kingside, mut black_queenside) =
        (false, false, false, false);
    for ch in field.chars() {
        match ch {
            'K' => white_kingside = true,
            'Q' => white_queenside = true,
            'k' => black_kingside = true,
            'q' => black_queenside = true,
            other => return Err(format!("invalid castling character: '{other}'")),
        }
    }

    Ok((white_kingside, white_queenside, black_kingside, black_queenside))
}

fn parse_en_passant(field: &str) -> Result<Option<Square>, String> {
    if field == "-" {
        return Ok(None);
    }

    let bytes = field.as_bytes();
    if bytes.len() != 2 {
        return Err(format!("invalid en passant square: '{field}'"));
    }
    let file = bytes[0].wrapping_sub(b'a');
    let rank = bytes[1].wrapping_sub(b'1');
    if file > 7 || rank > 7 {
        return Err(format!("invalid en passant square: '{field}'"));
    }

    Ok(Some(Square::new(file, rank)))
}
