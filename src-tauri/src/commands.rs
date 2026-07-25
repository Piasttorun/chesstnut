use std::sync::Mutex;

use chesstnut::engine::board::{Color, PieceKind, Square};
use chesstnut::engine::game::{Game, GameStatus};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct PieceDto {
    kind: &'static str,
    color: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockDto {
    white_ms: u64,
    black_ms: u64,
    increment_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameView {
    board: Vec<Option<PieceDto>>,
    turn: &'static str,
    status: &'static str,
    move_history: Vec<String>,
    fen: String,
    pgn: String,
    awaiting_clock_choice: bool,
    clock: Option<ClockDto>,
    // Centipawns from White's perspective — positive favors White. Always
    // present regardless of clock/AI settings, since it's a pure function
    // of the board (see chesstnut::ai::evaluate), not tied to either.
    score: i32,
}

/// A search result from the `analyze` command — deliberately not part of
/// `GameView`/`view()`, since a depth-N search is far more expensive than
/// everything else in that struct and `get_state` gets polled every 250ms
/// for the clock. The frontend calls `analyze` separately, only when the
/// position actually changes.
#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum ScoreDto {
    Centipawns(i32),
    MateIn(i32),
}

impl From<chesstnut::ai::Score> for ScoreDto {
    fn from(score: chesstnut::ai::Score) -> Self {
        match score {
            chesstnut::ai::Score::Centipawns(cp) => ScoreDto::Centipawns(cp),
            chesstnut::ai::Score::MateIn(n) => ScoreDto::MateIn(n),
        }
    }
}

fn kind_str(kind: PieceKind) -> &'static str {
    match kind {
        PieceKind::Pawn => "pawn",
        PieceKind::Knight => "knight",
        PieceKind::Bishop => "bishop",
        PieceKind::Rook => "rook",
        PieceKind::Queen => "queen",
        PieceKind::King => "king",
    }
}

fn color_str(color: Color) -> &'static str {
    match color {
        Color::White => "white",
        Color::Black => "black",
    }
}

fn status_str(status: GameStatus) -> &'static str {
    match status {
        GameStatus::InProgress => "in_progress",
        GameStatus::Check => "check",
        GameStatus::Checkmate => "checkmate",
        GameStatus::Stalemate => "stalemate",
        GameStatus::DrawByFiftyMoveRule => "draw_fifty_move",
        GameStatus::DrawByRepetition => "draw_repetition",
        GameStatus::Resignation => "resignation",
        GameStatus::Timeout => "timeout",
    }
}

fn view(game: &Game) -> GameView {
    let board = (0..64)
        .map(|index| {
            game.board()
                .get(Square::from_index(index))
                .map(|piece| PieceDto {
                    kind: kind_str(piece.kind),
                    color: color_str(piece.color),
                })
        })
        .collect();

    let clock = if game.is_clock_enabled() {
        Some(ClockDto {
            white_ms: game.remaining_ms(Color::White).unwrap_or(0),
            black_ms: game.remaining_ms(Color::Black).unwrap_or(0),
            increment_ms: game.increment_ms().unwrap_or(0),
        })
    } else {
        None
    };

    let chesstnut::ai::Score::Centipawns(score) = chesstnut::ai::evaluate(game.board()) else {
        unreachable!("evaluate() only ever produces Centipawns for now")
    };

    GameView {
        board,
        turn: color_str(game.turn()),
        status: status_str(game.status()),
        move_history: game.move_history().to_vec(),
        fen: game.to_fen(),
        pgn: game.to_pgn(),
        awaiting_clock_choice: game.awaiting_clock_choice(),
        clock,
        score,
    }
}

fn parse_square(text: &str) -> Result<Square, String> {
    let bytes = text.as_bytes();
    if bytes.len() != 2 {
        return Err(format!("invalid square: {text}"));
    }
    let file = bytes[0].wrapping_sub(b'a');
    let rank = bytes[1].wrapping_sub(b'1');
    if file > 7 || rank > 7 {
        return Err(format!("invalid square: {text}"));
    }
    Ok(Square::new(file, rank))
}

fn square_str(square: Square) -> String {
    format!("{}{}", (b'a' + square.file) as char, square.rank + 1)
}

/// Runs a depth-N search and returns its evaluation. Clones the position
/// out of the mutex and releases the lock immediately, rather than holding
/// it for the whole search — a slow deep search must never block
/// `make_move` (or anything else) from acquiring the lock while it runs.
///
/// Explicitly offloaded to Tauri's blocking thread pool via
/// `spawn_blocking` rather than left as a plain sync command — a sync
/// command still runs *somewhere*, and a multi-hundred-millisecond
/// CPU-bound search sharing a thread with IPC dispatch is exactly what
/// made move input feel laggy before this existed.
#[tauri::command]
pub async fn analyze(state: State<'_, Mutex<Game>>, depth: u32) -> Result<ScoreDto, String> {
    let game = state.lock().unwrap().clone();
    let score = tauri::async_runtime::spawn_blocking(move || chesstnut::ai::search(&game, depth))
        .await
        .map_err(|err| err.to_string())?;
    Ok(score.into())
}

#[tauri::command]
pub fn new_game(state: State<Mutex<Game>>) -> GameView {
    let mut game = state.lock().unwrap();
    *game = Game::new_pending_clock();
    view(&game)
}

/// Sets the time control for the game currently pending one — the frontend
/// calls this from the time-mode picker before the board becomes
/// interactive. `initial_ms: None` selects "No clock", which is a real,
/// explicit choice rather than a missing one.
#[tauri::command]
pub fn select_time_control(
    state: State<Mutex<Game>>,
    initial_ms: Option<u64>,
    increment_ms: u64,
) -> GameView {
    let mut game = state.lock().unwrap();
    game.select_time_control(initial_ms, increment_ms);
    view(&game)
}

#[tauri::command]
pub fn get_state(state: State<Mutex<Game>>) -> GameView {
    let game = state.lock().unwrap();
    view(&game)
}

#[tauri::command]
pub fn legal_moves(state: State<Mutex<Game>>, square: String) -> Result<Vec<String>, String> {
    let game = state.lock().unwrap();
    let from = parse_square(&square)?;

    if game.is_game_over() {
        return Ok(Vec::new());
    }

    let piece = match game.board().get(from) {
        Some(piece) => piece,
        None => return Ok(Vec::new()),
    };
    if piece.color != game.turn() {
        return Ok(Vec::new());
    }

    Ok(game
        .legal_moves_from(from)
        .into_iter()
        .map(|mv| square_str(mv.to))
        .collect())
}

fn parse_promotion(text: &str) -> Result<PieceKind, String> {
    match text {
        "queen" => Ok(PieceKind::Queen),
        "rook" => Ok(PieceKind::Rook),
        "bishop" => Ok(PieceKind::Bishop),
        "knight" => Ok(PieceKind::Knight),
        _ => Err(format!("invalid promotion piece: {text}")),
    }
}

/// Applies a move if legal. `legal_moves_from` generates queen/rook/bishop/
/// knight variants for the same (from, to) pair on a promotion — `promotion`
/// picks which; `None` defaults to whichever comes first (always the queen,
/// see rules::expand_promotions), for callers that don't care.
#[tauri::command]
pub fn make_move(
    state: State<Mutex<Game>>,
    from: String,
    to: String,
    promotion: Option<String>,
) -> Result<GameView, String> {
    let mut game = state.lock().unwrap();
    let from_square = parse_square(&from)?;
    let to_square = parse_square(&to)?;
    let promotion_kind = match promotion {
        Some(text) => Some(parse_promotion(&text)?),
        None => None,
    };

    let mut candidates = game
        .legal_moves_from(from_square)
        .into_iter()
        .filter(|mv| mv.to == to_square);

    let mv = match promotion_kind {
        Some(kind) => candidates
            .find(|mv| mv.promotion == Some(kind))
            .ok_or_else(|| "illegal move".to_string())?,
        None => candidates.next().ok_or_else(|| "illegal move".to_string())?,
    };

    game.make_move(mv).map_err(|_| "illegal move".to_string())?;
    Ok(view(&game))
}

#[tauri::command]
pub fn resign(state: State<Mutex<Game>>) -> Result<GameView, String> {
    let mut game = state.lock().unwrap();
    game.resign().map_err(|_| "the game is already over".to_string())?;
    Ok(view(&game))
}

#[tauri::command]
pub fn load_fen(state: State<Mutex<Game>>, fen: String) -> Result<GameView, String> {
    let mut game = state.lock().unwrap();
    *game = Game::from_fen(&fen)?;
    Ok(view(&game))
}

#[tauri::command]
pub fn load_pgn(state: State<Mutex<Game>>, pgn: String) -> Result<GameView, String> {
    let mut game = state.lock().unwrap();
    *game = Game::import_pgn(&pgn)?;
    Ok(view(&game))
}
