use std::time::Instant;

use crate::engine::board::{Board, Color, Piece, PieceKind, Square};
use crate::engine::fen;
use crate::engine::moves::Move;
use crate::engine::pgn;
use crate::engine::rules;
use crate::engine::zobrist;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameStatus {
    InProgress,
    Check,
    Checkmate,
    Stalemate,
    DrawByFiftyMoveRule,
    DrawByRepetition,
    Resignation,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimeControl {
    initial_ms: u64,
    increment_ms: u64,
}

// "No clock" is a real, deliberate choice, so it's a variant here rather
// than represented as `Option::None` — whether a choice has been made *at
// all* is tracked separately by `Game::awaiting_clock_choice`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClockMode {
    None,
    Timed(TimeControl),
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

// Clone is needed for search (ai::search clones the position at each node
// of the game tree rather than mutating and undoing moves in place — see
// that module for why simplicity won out over the speed of make/unmake).
#[derive(Clone)]
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
    // Set by `resign()`. Unlike every other terminal state, resignation
    // isn't derivable from the board at all — it's an explicit action — so
    // it needs its own flag rather than falling out of `status()`'s board
    // inspection.
    resigned: bool,
    // Blocks `make_move` until a time control has been chosen — see
    // `new_pending_clock`/`select_time_control`. `Game::new()` and friends
    // leave this `false` (clock defaults to `ClockMode::None`) so existing
    // callers — tests, FEN/PGN loading — behave exactly as before.
    awaiting_clock_choice: bool,
    clock_mode: ClockMode,
    white_remaining_ms: u64,
    black_remaining_ms: u64,
    // Wall-clock start of the side-to-move's current turn. `None` whenever
    // the clock isn't running (untimed game, or no choice made yet).
    turn_started_at: Option<Instant>,
    // Per-move clock reading parsed from a PGN's `[%clk ...]` comments during
    // import, parallel to `move_history` by index. Empty for games that
    // weren't imported from an annotated PGN. Store-only for now — nothing
    // reads this yet beyond `move_clock_log()` itself.
    move_clock_log: Vec<Option<u64>>,
    // Calendar date the game was created, for the PGN `[Date ...]` header.
    // Captured once at construction rather than at export time so a PGN
    // copied mid-game still reflects when the game actually started.
    created_date: String,
}

// PGN's own date format: "YYYY.MM.DD".
fn today_string() -> String {
    chrono::Local::now().format("%Y.%m.%d").to_string()
}

impl Game {
    pub fn new() -> Self {
        Game::from_board(Board::starting_position(), Color::White)
    }

    /// Like `new()`, but blocks `make_move` until `select_time_control` is
    /// called — the "you must pick a time control before the game can
    /// start" flow the desktop app uses for its New Game button.
    pub fn new_pending_clock() -> Self {
        let mut game = Game::new();
        game.awaiting_clock_choice = true;
        game
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
            resigned: false,
            awaiting_clock_choice: false,
            clock_mode: ClockMode::None,
            white_remaining_ms: 0,
            black_remaining_ms: 0,
            turn_started_at: None,
            move_clock_log: Vec::new(),
            created_date: today_string(),
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
            resigned: false,
            awaiting_clock_choice: false,
            clock_mode: ClockMode::None,
            white_remaining_ms: 0,
            black_remaining_ms: 0,
            turn_started_at: None,
            move_clock_log: Vec::new(),
            created_date: today_string(),
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
        let tokens = pgn::movetext_tokens_with_clock(text);
        for (token, _clock_ms) in &tokens {
            game.apply_san(token)?;
        }
        // Only assigned once every token has replayed successfully — a
        // failed import returns Err above and this line never runs, so
        // move_clock_log never ends up mismatched with move_history.
        game.move_clock_log = tokens.into_iter().map(|(_, clock_ms)| clock_ms).collect();
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

    /// A 64-bit fingerprint of everything that affects which moves are
    /// legal from here (board, whose turn it is, castling rights, en
    /// passant target) — used by the AI's transposition table (see
    /// `ai::search`) to recognize a position it's already searched. Two
    /// `Game`s with the same fen (ignoring move counters, which don't
    /// affect legality) always produce the same hash.
    pub fn zobrist_hash(&self) -> u64 {
        zobrist::hash(
            &self.board,
            self.turn,
            [
                self.castling_rights.white_kingside,
                self.castling_rights.white_queenside,
                self.castling_rights.black_kingside,
                self.castling_rights.black_queenside,
            ],
            self.en_passant_target,
        )
    }

    /// A lightweight, all-`Copy` snapshot of everything that determines
    /// legal moves — see [`Position`] for why the search (`ai::search`)
    /// recurses through these instead of cloning `Game` itself at every
    /// node.
    pub(crate) fn to_position(&self) -> Position {
        Position {
            board: self.board,
            turn: self.turn,
            castling_rights: self.castling_rights,
            en_passant_target: self.en_passant_target,
        }
    }

    pub fn move_history(&self) -> &[String] {
        &self.move_history
    }

    /// Per-move clock reading parsed from a `[%clk ...]`-annotated PGN
    /// import, parallel to `move_history` by index (`None` where a move had
    /// no clock comment). Empty for any game not loaded via `import_pgn`.
    pub fn move_clock_log(&self) -> &[Option<u64>] {
        &self.move_clock_log
    }

    pub fn awaiting_clock_choice(&self) -> bool {
        self.awaiting_clock_choice
    }

    pub fn is_clock_enabled(&self) -> bool {
        matches!(self.clock_mode, ClockMode::Timed(_))
    }

    pub fn increment_ms(&self) -> Option<u64> {
        match self.clock_mode {
            ClockMode::Timed(control) => Some(control.increment_ms),
            ClockMode::None => None,
        }
    }

    /// Sets the game's time control and unblocks `make_move`. `None` means
    /// "no clock" — a deliberate, explicit choice, not a default. Intended
    /// to be called once, before the first move; calling it again just
    /// restarts the clock from scratch, since there's no mid-game
    /// time-control-change flow to support.
    pub fn select_time_control(&mut self, initial_ms: Option<u64>, increment_ms: u64) {
        self.clock_mode = match initial_ms {
            Some(initial_ms) => {
                let control = TimeControl { initial_ms, increment_ms };
                self.white_remaining_ms = control.initial_ms;
                self.black_remaining_ms = control.initial_ms;
                self.turn_started_at = Some(Instant::now());
                ClockMode::Timed(control)
            }
            None => {
                self.turn_started_at = None;
                ClockMode::None
            }
        };
        self.awaiting_clock_choice = false;
    }

    /// Live remaining time for `color`, accounting for elapsed real time if
    /// it's currently that color's turn. `None` when there's no clock
    /// (untimed game, or no time control chosen yet).
    pub fn remaining_ms(&self, color: Color) -> Option<u64> {
        if !self.is_clock_enabled() {
            return None;
        }
        let stored = match color {
            Color::White => self.white_remaining_ms,
            Color::Black => self.black_remaining_ms,
        };
        if color != self.turn {
            return Some(stored);
        }
        match self.turn_started_at {
            Some(started_at) => Some(stored.saturating_sub(started_at.elapsed().as_millis() as u64)),
            None => Some(stored),
        }
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
        // Checkmate/Resignation/Timeout all share the "side to move is the
        // loser" convention `status()` and `resign()` already use, so the
        // winner is just `self.turn.opponent()` — no separate bookkeeping
        // for who resigned or got mated.
        let result = match self.status() {
            GameStatus::Checkmate | GameStatus::Resignation | GameStatus::Timeout => {
                match self.turn.opponent() {
                    Color::White => "1-0",
                    Color::Black => "0-1",
                }
            }
            GameStatus::Stalemate | GameStatus::DrawByFiftyMoveRule | GameStatus::DrawByRepetition => "1/2-1/2",
            GameStatus::InProgress | GameStatus::Check => "*",
        };

        // White/Black/Round stay placeholders — this is a local
        // pass-and-play game with no real event/player metadata to fill in.
        let date = &self.created_date;
        let headers = format!(
            "[Event \"Casual Game\"]\n\
             [Site \"Chesstnut\"]\n\
             [Date \"{date}\"]\n\
             [Round \"?\"]\n\
             [White \"?\"]\n\
             [Black \"?\"]\n\
             [Result \"{result}\"]\n\n"
        );
        let movetext = pgn::movetext(&self.move_history);
        if movetext.is_empty() {
            format!("{headers}{result}")
        } else {
            format!("{headers}{movetext} {result}")
        }
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

    /// All legal moves for the side currently to move, across every piece —
    /// what an AI opponent picks from, as opposed to `legal_moves_from`
    /// which is scoped to a single square for the UI's click-a-piece flow.
    pub fn legal_moves_for_turn(&self) -> Vec<Move> {
        self.all_legal_moves(self.turn)
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
        if self.resigned {
            return GameStatus::Resignation;
        }
        // Flagging takes priority over everything else — even a checkmate
        // sitting on the board doesn't matter if the clock already hit zero
        // first. `remaining_ms` clamps at zero via saturating_sub, so
        // `== Some(0)` reliably means "time's up", not "very low".
        if self.remaining_ms(self.turn) == Some(0) {
            return GameStatus::Timeout;
        }

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
                | GameStatus::Resignation
                | GameStatus::Timeout
        )
    }

    /// Ends the game immediately in favor of whoever is *not* currently to
    /// move — same "the side to move is the loser" convention `status()`
    /// already uses for checkmate, so callers infer the winner from `turn()`
    /// exactly the way they do today. Errs if the game already ended some
    /// other way.
    pub fn resign(&mut self) -> Result<(), IllegalMove> {
        if self.is_game_over() {
            return Err(IllegalMove);
        }
        self.resigned = true;
        Ok(())
    }

    pub fn make_move(&mut self, mv: Move) -> Result<(), IllegalMove> {
        if self.awaiting_clock_choice {
            return Err(IllegalMove);
        }
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

        update_castling_rights(&mut self.castling_rights, piece, mv);
        self.en_passant_target = en_passant_target_after(piece, mv);

        self.board = rules::apply_move(&self.board, mv);
        self.turn = self.turn.opponent();

        // Charge the mover for the time they just spent thinking, then
        // credit the increment (if any) and restart the clock for whoever's
        // turn it is now. `piece.color` — captured before `self.turn`
        // flipped above — is the side that just moved.
        if let ClockMode::Timed(control) = self.clock_mode {
            let started_at = self.turn_started_at.unwrap_or_else(Instant::now);
            let elapsed_ms = started_at.elapsed().as_millis() as u64;
            let mover_remaining = match piece.color {
                Color::White => &mut self.white_remaining_ms,
                Color::Black => &mut self.black_remaining_ms,
            };
            *mover_remaining = mover_remaining.saturating_sub(elapsed_ms) + control.increment_ms;
            self.turn_started_at = Some(Instant::now());
        }

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

}

/// Mutates `rights` for a piece having just moved from/to the two squares
/// `mv` names — a free function (rather than a `Game` method) so both
/// `Game::make_move` and `Position::make_move` can share it.
fn update_castling_rights(rights: &mut CastlingRights, piece: Piece, mv: Move) {
    if piece.kind == PieceKind::King {
        match piece.color {
            Color::White => {
                rights.white_kingside = false;
                rights.white_queenside = false;
            }
            Color::Black => {
                rights.black_kingside = false;
                rights.black_queenside = false;
            }
        }
    }

    // A rook leaving its home square, or being captured there, forfeits
    // that side's right either way — checking both `from` and `to`
    // catches both cases in one pass.
    for square in [mv.from, mv.to] {
        match (square.file, square.rank) {
            (0, 0) => rights.white_queenside = false,
            (7, 0) => rights.white_kingside = false,
            (0, 7) => rights.black_queenside = false,
            (7, 7) => rights.black_kingside = false,
            _ => {}
        }
    }
}

/// A lightweight, purely-mechanical snapshot of a position: the board,
/// whose turn it is, castling rights, and the en passant target square.
/// Nothing else `Game` tracks — move history, PGN/SAN bookkeeping, clocks,
/// repetition history, the fullmove counter — has any bearing on what
/// moves are legal or what a position is worth, so [`crate::ai::search`]
/// recurses through millions of these instead of millions of `Game`
/// clones. Every field here is `Copy`, so copying one is a plain stack
/// copy with no heap allocation at all — unlike cloning a `Game`, which
/// (via its `history`/`move_history`/`move_clock_log` `Vec`s and
/// `created_date` `String`) reallocates and copies data proportional to
/// how many moves the *real* game has played so far, at *every single
/// node* of the search tree. For a search several dozen moves into a real
/// game, that was easily the single largest cost per node — far more than
/// any search heuristic (transposition table, move ordering, pruning...)
/// changes, since none of those reduce the fixed cost paid *per node
/// visited*, only how many nodes get visited at all.
///
/// Deliberately doesn't track repetition or the fifty-move rule —
/// `ai::search` never inspected `Game::history`/`halfmove_clock` during a
/// search even before this change existed (see `negamax`, which detects
/// checkmate/stalemate directly from an empty move list rather than via
/// `Game::status()`), so leaving them out here costs nothing that was
/// actually working already. A position that's genuinely drawn by
/// repetition deep inside the search tree still isn't recognized as such
/// — a known limitation, not a regression from this change.
#[derive(Clone, Copy)]
pub(crate) struct Position {
    board: Board,
    turn: Color,
    castling_rights: CastlingRights,
    en_passant_target: Option<Square>,
}

impl Position {
    pub(crate) fn board(&self) -> &Board {
        &self.board
    }

    pub(crate) fn turn(&self) -> Color {
        self.turn
    }

    /// See `Game::zobrist_hash` — identical inputs, just read from this
    /// lighter-weight state instead.
    pub(crate) fn zobrist_hash(&self) -> u64 {
        zobrist::hash(
            &self.board,
            self.turn,
            [
                self.castling_rights.white_kingside,
                self.castling_rights.white_queenside,
                self.castling_rights.black_kingside,
                self.castling_rights.black_queenside,
            ],
            self.en_passant_target,
        )
    }

    /// See `Game::legal_moves_for_turn` — same rules (including castling
    /// and en passant), just against this lighter-weight state.
    pub(crate) fn legal_moves_for_turn(&self) -> Vec<Move> {
        let mut all = Vec::new();
        for index in 0..64 {
            let square = Square::from_index(index);
            if let Some(piece) = self.board.get(square) {
                if piece.color == self.turn {
                    all.extend(self.legal_moves_from(square));
                }
            }
        }
        all
    }

    fn legal_moves_from(&self, from: Square) -> Vec<Move> {
        let mut moves = rules::legal_moves_from(&self.board, from);

        for mv in en_passant_moves(&self.board, self.turn, self.en_passant_target) {
            if mv.from == from {
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
                        moves.push(mv);
                    }
                }
            }
        }

        moves
    }

    /// Applies `mv` in place. Unlike `Game::make_move`, infallible — every
    /// caller (see `ai::search`) only ever passes a move this exact
    /// position's own `legal_moves_for_turn` just produced, so there's
    /// nothing here to reject the way `Game::make_move` has to (a clock
    /// not yet chosen, the game already over, a move from a stale click)
    /// for moves arriving from outside the engine.
    pub(crate) fn make_move(&mut self, mv: Move) {
        let piece = self
            .board
            .get(mv.from)
            .expect("mv came from this position's own legal_moves_for_turn");
        update_castling_rights(&mut self.castling_rights, piece, mv);
        self.en_passant_target = en_passant_target_after(piece, mv);
        self.board = rules::apply_move(&self.board, mv);
        self.turn = self.turn.opponent();
    }

    /// A "pass" — flips whose turn it is without actually playing a move.
    /// Used only by null-move pruning (see `ai::search`): searching this
    /// position at a reduced depth answers "if my opponent could move
    /// twice in a row, would I still be fine?", and if the answer is yes,
    /// the real move is assumed to be at least that good without needing
    /// to search it in full. En passant rights lapse (no pawn just
    /// double-pushed to create them); everything else is unchanged.
    pub(crate) fn null_move(&self) -> Self {
        Position {
            turn: self.turn.opponent(),
            en_passant_target: None,
            ..*self
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
