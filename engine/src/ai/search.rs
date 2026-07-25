use crate::engine::board::Color;
use crate::engine::game::Game;
use crate::engine::moves::Move;
use crate::engine::rules;

use super::eval::piece_value;
use super::{evaluate, Score};

/// Large enough that no plausible material evaluation ever gets close to
/// it, so "the returned value is within a small distance of this" reliably
/// means "this line forces checkmate," not "someone's just up a lot of
/// material." Shrunk by `ply` at each mated leaf so the search prefers a
/// faster mate over a slower one (and, when forced to be mated, prefers
/// delaying it as long as possible).
const MATE_VALUE: i32 = 1_000_000;

/// Depth-limited minimax search with alpha-beta pruning, written in
/// negamax form (each side maximizes its own score, negating the child's
/// result rather than tracking separate max/min branches) — the same style
/// of engine chess programs have used since the 1980s. Returns a [`Score`]
/// from White's perspective, matching [`evaluate`], regardless of whose
/// turn it is at `game`'s position.
///
/// Two things keep this fast enough to be usable at the UI's depth range
/// without a transposition table or iterative deepening: move ordering
/// (captures first, biggest first — see `order_moves`) shrinks how much of
/// the tree alpha-beta has to look at in the first place, and the root
/// itself is split across every available CPU core (see
/// `best_root_score`), since the root is the one place a chess search can
/// be parallelized without needing to share alpha-beta state between
/// threads.
pub fn search(game: &Game, depth: u32) -> Score {
    if game.awaiting_clock_choice() {
        // Nothing to analyze before the game has actually started — and
        // `make_move` rejects everything in this state regardless of what
        // `legal_moves_for_turn` reports, so searching would just fail.
        return Score::Centipawns(0);
    }

    let depth = depth.max(1) as i32;
    let moves = order_moves(game, game.legal_moves_for_turn());

    let raw = if moves.is_empty() {
        if rules::is_in_check(game.board(), game.turn()) {
            -MATE_VALUE
        } else {
            0
        }
    } else {
        best_root_score(game, &moves, depth)
    };

    let white_perspective = match game.turn() {
        Color::White => raw,
        Color::Black => -raw,
    };
    to_score(white_perspective)
}

/// Searches every root move in parallel, one thread per available CPU core
/// (fewer if there are fewer moves than that). Each thread just needs the
/// best score among the moves it was handed — alpha-beta cutoffs still
/// happen normally *within* each thread's own subtree, they just aren't
/// shared *across* threads, which is what makes this safe to parallelize
/// without synchronization beyond collecting the results at the end.
fn best_root_score(game: &Game, moves: &[Move], depth: i32) -> i32 {
    let thread_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        // Leave at least one core free for the OS, the webview, and Tauri's
        // own IPC handling — using every last core turns a search into a
        // full CPU-saturation event with nothing left over to process a
        // click promptly, which is exactly the "moves feel laggy" bug this
        // is fixing, not a hypothetical one.
        .saturating_sub(1)
        .max(1)
        .min(moves.len());
    let chunk_size = moves.len().div_ceil(thread_count);

    std::thread::scope(|scope| {
        moves
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .filter_map(|&mv| {
                            let mut next = game.clone();
                            next.make_move(mv).ok()?;
                            Some(-negamax(&next, depth - 1, 1, -(MATE_VALUE + 1), MATE_VALUE + 1))
                        })
                        .max()
                        .unwrap_or(-(MATE_VALUE + 1))
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().expect("search thread panicked"))
            .max()
            .unwrap_or(-(MATE_VALUE + 1))
    })
}

/// Returns a score from the perspective of whoever is to move in `game` —
/// positive is good for the mover, regardless of color.
fn negamax(game: &Game, depth: i32, ply: i32, mut alpha: i32, beta: i32) -> i32 {
    let moves = order_moves(game, game.legal_moves_for_turn());

    if moves.is_empty() {
        return if rules::is_in_check(game.board(), game.turn()) {
            // The mover is checkmated — as bad as it gets, but a mate found
            // deeper in the tree (larger `ply`) is less bad than an
            // immediate one, so the search picks the longest defense when
            // a loss is unavoidable.
            -(MATE_VALUE - ply)
        } else {
            0 // stalemate
        };
    }

    if depth == 0 {
        return mover_relative_eval(game);
    }

    let mut best = -(MATE_VALUE + 1);
    for mv in moves {
        let mut next = game.clone();
        if next.make_move(mv).is_err() {
            // `legal_moves_for_turn` is a pure board check and doesn't know
            // about `make_move`'s other guards (clock choice pending, or —
            // for a live timed game — real wall-clock time running out
            // while a slow deep search is still thinking). Both are rare
            // and effectively mean "this position isn't playable anymore,"
            // so skip the move rather than let a stale `Ok` assumption
            // crash the whole app.
            continue;
        }
        let score = -negamax(&next, depth - 1, ply + 1, -beta, -alpha);
        if score > best {
            best = score;
        }
        if best > alpha {
            alpha = best;
        }
        if alpha >= beta {
            break; // alpha-beta cutoff — the opponent already has a better option elsewhere
        }
    }
    best
}

/// Sorts captures before quiet moves, biggest capture first (an
/// approximation of the classic MVV-LVA heuristic without weighing the
/// attacker, just the victim). This doesn't change what the search finds —
/// alpha-beta still visits every node it would have anyway — but trying
/// the most promising moves first means the cutoffs in `negamax` trigger
/// much earlier, so a large chunk of the tree never gets visited at all.
fn order_moves(game: &Game, mut moves: Vec<Move>) -> Vec<Move> {
    moves.sort_by_key(|mv| {
        let captured_value = game.board().get(mv.to).map(|p| piece_value(p.kind)).unwrap_or(0);
        std::cmp::Reverse(captured_value)
    });
    moves
}

fn mover_relative_eval(game: &Game) -> i32 {
    let Score::Centipawns(white_score) = evaluate(game.board()) else {
        unreachable!("evaluate() only ever produces Centipawns for now")
    };
    match game.turn() {
        Color::White => white_score,
        Color::Black => -white_score,
    }
}

/// Converts an internal White-perspective value back into the public
/// [`Score`] type, recognizing mate-distance values and turning them into
/// `MateIn` (plies converted to full moves, rounded up — mate "in 3" means
/// White or Black needs at most 3 of their own moves, not 3 plies).
fn to_score(white_perspective: i32) -> Score {
    let magnitude = white_perspective.abs();
    // Any ordinary material evaluation stays far below this — the gap only
    // closes when `negamax` actually returned a mate-distance value.
    if magnitude > MATE_VALUE - 1000 {
        let plies_to_mate = MATE_VALUE - magnitude;
        let moves_to_mate = (plies_to_mate + 1) / 2;
        Score::MateIn(if white_perspective > 0 { moves_to_mate } else { -moves_to_mate })
    } else {
        Score::Centipawns(white_perspective)
    }
}
