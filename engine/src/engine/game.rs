use crate::engine::board::{Board, Color, Piece, PieceKind, Square};
use crate::engine::fen;
use crate::engine::moves::Move;
use crate::engine::pgn;
use crate::engine::rules;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameStatus {
    InProgress,
    Check,
    Checkmate,
    Stalemate,
    DrawByFiftyMoveRule,
    DrawByRepetition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IllegalMove;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CastlingRights {
    white_kingside: bool,
    white_queenside: bool,
    black_kingside: bool,
    black_queenside: bool,
}

impl CastlingRights {
    fn all() -> Self {
        CastlingRights {
            white_kingside: true,
            white_queenside: true,
            black_kingside: true,
            black_queenside: true,
        }
    }
}

pub struct Game {
    board: Board,
    turn: Color,
    castling_rights: CastlingRights,
    en_passant_target: Option<Square>,
    halfmove_clock: u32,
    // Every position reached so far (including the starting one), for
    // threefold-repetition detection. A plain Vec + linear scan is fine at
    // this board size — see the "does reconstructing the board matter"
    // discussion from earlier: copying/comparing a 64-square board is cheap.
    history: Vec<(Board, Color)>,
    // SAN strings in play order, one per move, for PGN export.
    move_history: Vec<String>,
    // FEN's move counter: starts at 1, increments after Black replies.
    // Stored explicitly rather than derived from history.len() because
    // from_fen() can start a game already partway through, where that
    // derivation would be wrong.
    fullmove_number: u32,
}

impl Game {
    pub fn new() -> Self {
        Game::from_board(Board::starting_position(), Color::White)
    }

    pub fn from_board(board: Board, turn: Color) -> Self {
        let mut game = Game {
            board,
            turn,
            castling_rights: CastlingRights::all(),
            en_passant_target: None,
            halfmove_clock: 0,
            history: Vec::new(),
            move_history: Vec::new(),
            fullmove_number: 1,
        };
        game.history.push((game.board, game.turn));
        game
    }

    /// Loads a position from a FEN string. Only syntax is validated (right
    /// number of fields, legal characters, ranks summing to 8 squares) —
    /// not chess legality, so e.g. a FEN with two white kings will load
    /// without complaint. Malformed input fails with a plain error string;
    /// there's no attempt to partially recover.
    pub fn from_fen(text: &str) -> Result<Self, String> {
        let parsed = fen::parse(text)?;
        let mut game = Game {
            board: parsed.board,
            turn: parsed.turn,
            castling_rights: CastlingRights {
                white_kingside: parsed.white_kingside,
                white_queenside: parsed.white_queenside,
                black_kingside: parsed.black_kingside,
                black_queenside: parsed.black_queenside,
            },
            en_passant_target: parsed.en_passant_target,
            halfmove_clock: parsed.halfmove_clock,
            history: Vec::new(),
            move_history: Vec::new(),
            fullmove_number: parsed.fullmove_number,
        };
        game.history.push((game.board, game.turn));
        Ok(game)
    }

    /// Replays a game from PGN movetext, always starting from the standard
    /// starting position (a `[FEN ...]` header for non-standard starts
    /// isn't supported). Each token is matched against that position's
    /// actual legal moves rather than hand-parsed — see pgn::movetext_tokens
    /// and apply_san below. The first token that doesn't match a legal move
    /// fails the whole import; there's no partial-game recovery.
    pub fn import_pgn(text: &str) -> Result<Self, String> {
        let mut game = Game::new();
        for token in pgn::movetext_tokens(text) {
            game.apply_san(&token)?;
        }
        Ok(game)
    }

    fn apply_san(&mut self, token: &str) -> Result<(), String> {
        let candidates = self.all_legal_moves(self.turn);
        let board_before = self.board;
        let mover_color = self.turn;

        let matches: Vec<Move> = candidates
            .iter()
            .copied()
            .filter(|&mv| pgn::san(&board_before, mover_color, &candidates, mv, false, false) == token)
            .collect();

        match matches.as_slice() {
            [mv] => self
                .make_move(*mv)
                .map_err(|_| format!("move '{token}' matched a legal move but was rejected")),
            [] => Err(format!("no legal move matches '{token}'")),
            _ => Err(format!("move '{token}' is ambiguous")),
        }
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn turn(&self) -> Color {
        self.turn
    }

    pub fn move_history(&self) -> &[String] {
        &self.move_history
    }

    pub fn to_fen(&self) -> String {
        fen::format(
            &self.board,
            self.turn,
            self.castling_rights.white_kingside,
            self.castling_rights.white_queenside,
            self.castling_rights.black_kingside,
            self.castling_rights.black_queenside,
            self.en_passant_target,
            self.halfmove_clock,
            self.fullmove_number,
        )
    }

    pub fn to_pgn(&self) -> String {
        // Placeholder header values — this is a local pass-and-play game
        // with no event/player metadata to fill in for real.
        let headers = "[Event \"Casual Game\"]\n\
             [Site \"Chesstnut\"]\n\
             [Date \"????.??.??\"]\n\
             [Round \"?\"]\n\
             [White \"?\"]\n\
             [Black \"?\"]\n\
             [Result \"*\"]\n\n";
        format!("{headers}{}", pgn::movetext(&self.move_history))
    }

    /// All legal moves for the piece on `from`, including castling and en
    /// passant when applicable. Castling and en passant can't live in
    /// `rules::legal_moves_from` — that function only sees a `Board`, but
    /// both moves depend on state a bare board can't express (has this king
    /// or rook ever moved? was the last move a two-square pawn push?), which
    /// is exactly the state `Game` carries.
    pub fn legal_moves_from(&self, from: Square) -> Vec<Move> {
        let mut moves = rules::legal_moves_from(&self.board, from);

        for mv in en_passant_moves(&self.board, self.turn, self.en_passant_target) {
            if mv.from == from {
                // En passant can expose a discovered check (a pawn pinned
                // along the rank), so it needs the same check-safety
                // simulation as any other move.
                let hypothetical = rules::apply_move(&self.board, mv);
                if !rules::is_in_check(&hypothetical, self.turn) {
                    moves.push(mv);
                }
            }
        }

        if let Some(piece) = self.board.get(from) {
            if piece.kind == PieceKind::King {
                for mv in castling_moves(&self.board, self.turn, self.castling_rights) {
                    if mv.from == from {
                        // Unlike normal moves, castling_moves() already
                        // verifies the king isn't in, through, or landing in
                        // check itself, so it skips the generic filter above.
                        moves.push(mv);
                    }
                }
            }
        }

        moves
    }

    fn all_legal_moves(&self, color: Color) -> Vec<Move> {
        let mut all = Vec::new();
        for index in 0..64 {
            let square = Square::from_index(index);
            if let Some(piece) = self.board.get(square) {
                if piece.color == color {
                    all.extend(self.legal_moves_from(square));
                }
            }
        }
        all
    }

    fn is_threefold_repetition(&self) -> bool {
        let current = (self.board, self.turn);
        self.history.iter().filter(|&&entry| entry == current).count() >= 3
    }

    pub fn status(&self) -> GameStatus {
        let in_check = rules::is_in_check(&self.board, self.turn);
        let has_moves = !self.all_legal_moves(self.turn).is_empty();

        if in_check && !has_moves {
            return GameStatus::Checkmate;
        }
        if !in_check && !has_moves {
            return GameStatus::Stalemate;
        }
        // Simplified vs. real chess: FIDE rules let a player *claim* these
        // as a draw rather than ending the game automatically. Treating them
        // as automatic is a deliberate simplification for a first version.
        if self.halfmove_clock >= 100 {
            return GameStatus::DrawByFiftyMoveRule;
        }
        if self.is_threefold_repetition() {
            return GameStatus::DrawByRepetition;
        }
        if in_check {
            return GameStatus::Check;
        }
        GameStatus::InProgress
    }

    /// True once the game has reached a terminal state. Checkmate and
    /// stalemate already block every move on their own (there are none left
    /// to find), but the fifty-move and repetition draws don't zero out the
    /// position's legal moves — the pieces can still physically move, the
    /// *game* is just over — so `make_move` needs this as an explicit guard.
    pub fn is_game_over(&self) -> bool {
        matches!(
            self.status(),
            GameStatus::Checkmate
                | GameStatus::Stalemate
                | GameStatus::DrawByFiftyMoveRule
                | GameStatus::DrawByRepetition
        )
    }

    pub fn make_move(&mut self, mv: Move) -> Result<(), IllegalMove> {
        if self.is_game_over() {
            return Err(IllegalMove);
        }

        let piece = self.board.get(mv.from).ok_or(IllegalMove)?;
        if piece.color != self.turn {
            return Err(IllegalMove);
        }

        if !self.legal_moves_from(mv.from).contains(&mv) {
            return Err(IllegalMove);
        }

        let is_pawn_move = piece.kind == PieceKind::Pawn;
        // En passant captures land on an empty square, so the plain
        // "something was on `to`" check alone would miss it.
        let is_capture = self.board.get(mv.to).is_some()
            || (is_pawn_move && mv.from.file != mv.to.file);

        // SAN needs the position *before* the move (to know what's being
        // captured and which same-kind pieces could also reach `mv.to`, for
        // disambiguation) — both captured here before anything mutates.
        let board_before = self.board;
        let legal_moves_before = self.all_legal_moves(self.turn);

        self.update_castling_rights(piece, mv);
        self.en_passant_target = en_passant_target_after(piece, mv);

        self.board = rules::apply_move(&self.board, mv);
        self.turn = self.turn.opponent();
        self.halfmove_clock = if is_pawn_move || is_capture {
            0
        } else {
            self.halfmove_clock + 1
        };
        if piece.color == Color::Black {
            self.fullmove_number += 1;
        }
        self.history.push((self.board, self.turn));

        let status_after = self.status();
        let is_check = matches!(status_after, GameStatus::Check | GameStatus::Checkmate);
        let is_checkmate = matches!(status_after, GameStatus::Checkmate);
        self.move_history.push(pgn::san(
            &board_before,
            piece.color,
            &legal_moves_before,
            mv,
            is_check,
            is_checkmate,
        ));

        Ok(())
    }

    fn update_castling_rights(&mut self, piece: Piece, mv: Move) {
        if piece.kind == PieceKind::King {
            match piece.color {
                Color::White => {
                    self.castling_rights.white_kingside = false;
                    self.castling_rights.white_queenside = false;
                }
                Color::Black => {
                    self.castling_rights.black_kingside = false;
                    self.castling_rights.black_queenside = false;
                }
            }
        }

        // A rook leaving its home square, or being captured there, forfeits
        // that side's right either way — checking both `from` and `to`
        // catches both cases in one pass.
        for square in [mv.from, mv.to] {
            match (square.file, square.rank) {
                (0, 0) => self.castling_rights.white_queenside = false,
                (7, 0) => self.castling_rights.white_kingside = false,
                (0, 7) => self.castling_rights.black_queenside = false,
                (7, 7) => self.castling_rights.black_kingside = false,
                _ => {}
            }
        }
    }
}

fn en_passant_target_after(piece: Piece, mv: Move) -> Option<Square> {
    if piece.kind != PieceKind::Pawn {
        return None;
    }
    let rank_delta = mv.to.rank as i8 - mv.from.rank as i8;
    if rank_delta == 2 {
        Some(Square::new(mv.from.file, mv.from.rank + 1))
    } else if rank_delta == -2 {
        Some(Square::new(mv.from.file, mv.from.rank - 1))
    } else {
        None
    }
}

fn en_passant_moves(board: &Board, turn: Color, target: Option<Square>) -> Vec<Move> {
    let target = match target {
        Some(square) => square,
        None => return Vec::new(),
    };

    let direction: i8 = match turn {
        Color::White => 1,
        Color::Black => -1,
    };
    let capturing_rank = (target.rank as i8 - direction) as u8;

    let mut moves = Vec::new();
    for file_delta in [-1i8, 1i8] {
        let from_file = target.file as i8 - file_delta;
        if from_file < 0 || from_file > 7 {
            continue;
        }
        let from = Square::new(from_file as u8, capturing_rank);
        if board.get(from)
            == Some(Piece {
                kind: PieceKind::Pawn,
                color: turn,
            })
        {
            moves.push(Move::new(from, target));
        }
    }
    moves
}

fn castling_moves(board: &Board, turn: Color, rights: CastlingRights) -> Vec<Move> {
    let mut moves = Vec::new();
    let rank = match turn {
        Color::White => 0,
        Color::Black => 7,
    };

    let king_square = Square::new(4, rank);
    let expected_king = Some(Piece {
        kind: PieceKind::King,
        color: turn,
    });
    if board.get(king_square) != expected_king {
        return moves;
    }
    if rules::is_square_attacked(board, king_square, turn.opponent()) {
        return moves;
    }

    let expected_rook = Some(Piece {
        kind: PieceKind::Rook,
        color: turn,
    });
    let (kingside_right, queenside_right) = match turn {
        Color::White => (rights.white_kingside, rights.white_queenside),
        Color::Black => (rights.black_kingside, rights.black_queenside),
    };

    if kingside_right {
        let rook_square = Square::new(7, rank);
        let path = [Square::new(5, rank), Square::new(6, rank)];
        let clear = path.iter().all(|&sq| board.get(sq).is_none());
        let safe = path
            .iter()
            .all(|&sq| !rules::is_square_attacked(board, sq, turn.opponent()));
        if board.get(rook_square) == expected_rook && clear && safe {
            moves.push(Move::new(king_square, Square::new(6, rank)));
        }
    }

    if queenside_right {
        let rook_square = Square::new(0, rank);
        let empty_path = [Square::new(1, rank), Square::new(2, rank), Square::new(3, rank)];
        let king_path = [Square::new(2, rank), Square::new(3, rank)];
        let clear = empty_path.iter().all(|&sq| board.get(sq).is_none());
        let safe = king_path
            .iter()
            .all(|&sq| !rules::is_square_attacked(board, sq, turn.opponent()));
        if board.get(rook_square) == expected_rook && clear && safe {
            moves.push(Move::new(king_square, Square::new(2, rank)));
        }
    }

    moves
}
