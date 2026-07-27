use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::engine::board::{Color, PieceKind, Square};
use crate::engine::game::Game;
use crate::engine::moves::Move;
use crate::engine::rules;

use super::eval::piece_value;
use super::{evaluate, Engine, Score};

/// Which kind of score a `TranspositionTable` entry actually represents —
/// alpha-beta pruning means most nodes never get searched enough to know
/// their *exact* value, only a bound on it (see `negamax`'s comment on
/// where each variant comes from).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bound {
    /// The stored score is the position's true value.
    Exact,
    /// The true score is at least this — this node caused a beta cutoff,
    /// so sibling moves that would have narrowed it further were never
    /// explored.
    Lower,
    /// The true score is at most this — every move was searched and none
    /// improved alpha, so nothing better than this was found (but a wider
    /// window might have found something worse instead).
    Upper,
}

#[derive(Clone, Copy)]
struct TtEntry {
    // Stored alongside the score so a lookup can confirm this slot really
    // is the position being asked about, not a different one that happens
    // to hash to the same index (see TABLE_SIZE below).
    hash: u64,
    depth: i32,
    score: i32,
    bound: Bound,
    best_move: Option<Move>,
}

// 2^20 slots — a few tens of MB at this entry size, enough to hold most of
// what a depth-6-ish search touches without the multi-hundred-MB tables a
// "real" engine budgets. Not tuned against real data yet; a reasonable
// starting point rather than a carefully measured one.
const TABLE_SIZE: usize = 1 << 20;

/// A depth-and-bound-aware cache from position (see `Game::zobrist_hash`)
/// to previously computed search results, so a subtree reached by a
/// different move order than the one that first explored it doesn't get
/// fully re-searched from scratch. Created once per `think()` call and
/// shared across that call's entire iterative-deepening ladder (see
/// `think`) — depth 1's results are still sitting in the table by the time
/// depth 2 starts, and so on — but not persisted beyond that one call: the
/// position is guaranteed unchanged for the table's whole lifetime that
/// way, with no staleness bookkeeping needed at all. Persisting it across
/// separate `analyze`/`request_ai_move` calls (i.e., across a player's
/// actual moves) would need real invalidation logic and is a next-step
/// improvement, not this one.
///
/// Guarded by a single `Mutex` rather than a lock-free scheme: `best_root_score`
/// caps itself at a handful of threads, and every node already pays for a
/// fresh legal-move generation pass (this engine clones the board and
/// regenerates moves rather than doing make/unmake — see `Game`'s own doc
/// comment on why) — lock contention on a Vec index is not the bottleneck
/// here, and a lock-free table is real engineering complexity this project
/// doesn't need yet.
pub(crate) struct TranspositionTable {
    entries: Mutex<Vec<Option<TtEntry>>>,
}

impl TranspositionTable {
    fn new() -> Self {
        TranspositionTable { entries: Mutex::new(vec![None; TABLE_SIZE]) }
    }

    fn slot(hash: u64) -> usize {
        (hash as usize) % TABLE_SIZE
    }

    /// Looks up `hash`, returning a score usable at `depth`/`alpha`/`beta`
    /// if the table has one (`None` otherwise), plus the entry's stored
    /// move regardless — even when the score itself isn't deep enough to
    /// trust, the move it was best last time is still a good ordering hint
    /// (see `order_moves`'s `preferred` parameter).
    fn probe(&self, hash: u64, depth: i32, alpha: i32, beta: i32) -> (Option<i32>, Option<Move>) {
        let entries = self.entries.lock().unwrap();
        let Some(entry) = entries[Self::slot(hash)] else {
            return (None, None);
        };
        if entry.hash != hash {
            return (None, None); // a different position landed on the same slot
        }
        if entry.depth < depth {
            return (None, entry.best_move); // not searched deep enough to trust the score
        }
        let usable_score = match entry.bound {
            Bound::Exact => Some(entry.score),
            Bound::Lower if entry.score >= beta => Some(entry.score),
            Bound::Upper if entry.score <= alpha => Some(entry.score),
            _ => None,
        };
        (usable_score, entry.best_move)
    }

    /// Always-replace: the simplest possible policy, and fine at this
    /// scale — a deeper, more valuable entry occasionally getting evicted
    /// by a shallower one from an unrelated position is the cost, but a
    /// fancier replacement scheme (e.g. "only overwrite if the new depth
    /// is >= the old one") is more complexity for a benefit this project
    /// hasn't needed to chase yet.
    fn store(&self, hash: u64, depth: i32, score: i32, bound: Bound, best_move: Option<Move>) {
        let mut entries = self.entries.lock().unwrap();
        entries[Self::slot(hash)] = Some(TtEntry { hash, depth, score, bound, best_move });
    }
}

// Comfortably deeper than iterative deepening's target depth plus the
// quiescence and null-move extensions ever push `ply` in practice at the
// depths this app uses — a killer recorded past this just falls in the
// last slot instead of its own, which only costs a little ordering
// quality, never correctness.
const MAX_KILLER_PLY: usize = 64;

/// Two "killer" quiet moves per ply — quiet (non-capture) moves that
/// caused a beta cutoff somewhere in the tree at that ply before, so
/// trying them first at a sibling node (same ply, different position —
/// unlike the transposition table, this is indexed by ply, not by
/// position) is likely to cut off quickly there too. `order_moves`
/// already puts captures first via a material-value heuristic; this is
/// what gives the many *quiet* moves it otherwise leaves unordered a
/// reasonable first guess instead of none at all. Scoped to one `think()`
/// call, same as `TranspositionTable` and for the same reason: a cutoff
/// from a position the player has since moved on from isn't worth
/// remembering.
pub(crate) struct KillerMoves {
    slots: Mutex<Vec<[Option<Move>; 2]>>,
}

impl KillerMoves {
    fn new() -> Self {
        KillerMoves {
            slots: Mutex::new(vec![[None, None]; MAX_KILLER_PLY]),
        }
    }

    fn get(&self, ply: i32) -> [Option<Move>; 2] {
        let index = (ply as usize).min(MAX_KILLER_PLY - 1);
        self.slots.lock().unwrap()[index]
    }

    /// Records `mv` as the most recent killer at `ply`, bumping whatever
    /// was already in slot 0 down to slot 1 — a small two-entry
    /// most-recent list, not a frequency count. A no-op if `mv` is
    /// already slot 0's occupant, so a move that keeps causing cutoffs at
    /// this ply doesn't shuffle itself into slot 1 and back out pointlessly.
    fn record(&self, ply: i32, mv: Move) {
        let index = (ply as usize).min(MAX_KILLER_PLY - 1);
        let mut slots = self.slots.lock().unwrap();
        let pair = &mut slots[index];
        if pair[0] == Some(mv) {
            return;
        }
        pair[1] = pair[0];
        pair[0] = Some(mv);
    }
}

/// Lets a search notice mid-flight that the position it's analyzing is no
/// longer the current one — the player made another move (or started a new
/// game, loaded a FEN, ...) while a slow, deep search from the *previous*
/// position was still running — and stop early instead of burning CPU on a
/// result the frontend already knows to discard. Cutting thread count
/// (see `best_root_score`) only limits how much of the machine one search
/// can claim; it doesn't stop a stale one from running to completion and
/// competing with the next click for however long that takes. Cloneable
/// and cheap to pass around — it's just an `Arc` clone plus one integer.
#[derive(Clone)]
pub struct Cancellation {
    tracking: Option<(Arc<AtomicU64>, u64)>,
}

impl Cancellation {
    /// A search that never gets cancelled — what the plain [`search`]
    /// entry point (used directly by tests, and anywhere else that isn't
    /// wiring this up to a live position) passes.
    pub fn none() -> Self {
        Cancellation { tracking: None }
    }

    /// `generation` is a counter the caller bumps every time the tracked
    /// position actually changes; `expected` is its value at the moment
    /// this search started. Once they no longer match, the position has
    /// moved on.
    pub fn tracking(generation: Arc<AtomicU64>, expected: u64) -> Self {
        Cancellation { tracking: Some((generation, expected)) }
    }

    fn is_cancelled(&self) -> bool {
        match &self.tracking {
            Some((generation, expected)) => generation.load(Ordering::Relaxed) != *expected,
            None => false,
        }
    }
}

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
/// Several things keep this fast enough to be usable at the UI's depth
/// range: move ordering (the TT move, then captures, then killer moves
/// first — see `order_moves`) shrinks how much of the tree alpha-beta has
/// to look at in the first place; iterative deepening (searching depth 1,
/// 2, ..., then the target depth — see the loop in `search_cancellable`)
/// is what supplies that "previous depth's best move" ordering hint, and
/// also what aspiration windows (see `search_root_with_aspiration`) use to
/// start each depth's root search with a narrow, cheap-to-prune guess at
/// the score instead of a wide-open one; a transposition table (see
/// `TranspositionTable`) avoids re-searching a subtree reached by a
/// different move order than the one that first found it; killer moves
/// (see `KillerMoves`) give the many quiet (non-capture) moves that
/// ordering otherwise leaves untouched a reasonable first guess too;
/// principal variation search (see the scout-then-re-search logic in
/// `negamax`'s move loop) spends full-width alpha-beta effort only on the
/// move actually expected to be best, and a cheap null-window check on
/// every sibling; null-move pruning (see the guard in `negamax`) skips
/// whole subtrees the opponent couldn't escape even given a free move; and
/// the root itself is split across every available CPU core (see
/// `best_root_score`), since the root is the one place a chess search can
/// be parallelized without needing to share alpha-beta state between
/// threads.
pub fn search(game: &Game, depth: u32) -> Score {
    search_cancellable(game, depth, &Cancellation::none())
}

/// Same as [`search`], but stops early — returning whatever partial result
/// it has, which the caller is expected to discard rather than display —
/// once `cancel` reports the position it was analyzing is stale. See
/// [`Cancellation`].
pub fn search_cancellable(game: &Game, depth: u32, cancel: &Cancellation) -> Score {
    think(game, depth, cancel).0
}

/// The move [`search`] would score, if `game` actually has one — `None`
/// only when the game is already over (checkmate/stalemate) or a time
/// control hasn't been chosen yet. This is what a caller actually playing
/// the move (as opposed to just displaying an evaluation, like the eval
/// bar does) needs.
///
/// Checks [`super::book_move`] first — deliberately not applied to
/// [`search`]/[`search_cancellable`] above, which are about *evaluating*
/// the position honestly, not about what's actually best to play. A
/// material-only eval at these depths can't tell a well-known strong
/// opening move from a passive one, so without this the engine would
/// happily search its way into something like an early flank pawn push
/// purely because nothing in its evaluation function knows better yet.
pub fn best_move(game: &Game, depth: u32) -> Option<Move> {
    best_move_cancellable(game, depth, &Cancellation::none())
}

/// Cancellable counterpart to [`best_move`] — see [`Cancellation`] and
/// [`search_cancellable`].
pub fn best_move_cancellable(game: &Game, depth: u32, cancel: &Cancellation) -> Option<Move> {
    super::book_move(game).or_else(|| think(game, depth, cancel).1)
}

/// This search, at a fixed depth, as an [`Engine`] — the interchangeable
/// "an opponent that picks a move" interface [`RandomEngine`](super::RandomEngine)
/// already implements. What plays the "computer" side once a play-vs-AI
/// mode calls `Engine::choose_move` instead of the eval bar calling
/// [`search`] directly.
pub struct SearchEngine {
    pub depth: u32,
}

impl Engine for SearchEngine {
    fn choose_move(&self, game: &Game) -> Option<Move> {
        best_move(game, self.depth)
    }
}

/// Shared implementation behind [`search_cancellable`] and
/// [`best_move_cancellable`] — one search, both the evaluation and the move
/// that achieves it are usually computed together anyway, so there's no
/// reason to run it twice for callers that want both (a future "AI plays a
/// move and the eval bar shows what it thinks of the result" flow, say).
fn think(game: &Game, depth: u32, cancel: &Cancellation) -> (Score, Option<Move>) {
    if game.awaiting_clock_choice() {
        // Nothing to analyze before the game has actually started — and
        // `make_move` rejects everything in this state regardless of what
        // `legal_moves_for_turn` reports, so searching would just fail.
        return (Score::Centipawns(0), None);
    }

    let depth = depth.max(1) as i32;
    let mut moves = order_moves(game, game.legal_moves_for_turn(), None, [None, None]);

    let (raw, best_move) = if moves.is_empty() {
        let raw = if rules::is_in_check(game.board(), game.turn()) {
            -MATE_VALUE
        } else {
            0
        };
        (raw, None)
    } else {
        // One table, shared across every depth in the ladder below — see
        // TranspositionTable's own doc comment for why its lifetime is
        // scoped to exactly this call.
        let tt = TranspositionTable::new();
        let killers = KillerMoves::new();

        // Iterative deepening: search depth 1, then 2, ..., then the
        // requested depth, instead of jumping straight there. Each
        // shallower pass is cheap (exponentially fewer nodes than the
        // final one) and its best move gets tried first at the next depth
        // — a good first move triggers alpha-beta cutoffs in its siblings
        // almost immediately, which consistently prunes far more of the
        // tree than searching the target depth cold ever does. The net
        // effect is usually faster overall despite nominally doing more
        // search passes, which is why essentially every real engine does
        // this rather than searching one depth in isolation.
        let mut raw = -(MATE_VALUE + 1);
        let mut best_move = moves[0];
        for current_depth in 1..=depth {
            if cancel.is_cancelled() {
                break;
            }
            // Aspiration windows: every depth after the first has a
            // genuine prior guess at the score (the previous, shallower
            // depth's result) — a real move rarely swings the position's
            // value by much versus one ply less of lookahead, so starting
            // alpha-beta narrowly around that guess (see
            // search_root_with_aspiration) lets far more of the tree get
            // cut off than the wide-open window the very first depth has
            // no choice but to use.
            let (score, mv) = search_root_with_aspiration(
                game,
                &moves,
                current_depth,
                cancel,
                &tt,
                &killers,
                raw,
                current_depth > 1,
            );
            if cancel.is_cancelled() {
                // This pass was cut short partway through — its score
                // reflects only some of the root moves, not all of them,
                // so the previous *completed* depth's result is more
                // trustworthy than adopting this partial one.
                break;
            }
            raw = score;
            best_move = mv;
            moves = reorder_with_preference(moves, best_move);
        }
        (raw, Some(best_move))
    };

    let white_perspective = match game.turn() {
        Color::White => raw,
        Color::Black => -raw,
    };
    (to_score(white_perspective), best_move)
}

/// Moves `preferred` to the front of `moves`, leaving the rest in place —
/// used between iterative-deepening passes so the next (deeper) pass tries
/// the previous pass's best move first.
fn reorder_with_preference(mut moves: Vec<Move>, preferred: Move) -> Vec<Move> {
    if let Some(pos) = moves.iter().position(|&mv| mv == preferred) {
        moves.swap(0, pos);
    }
    moves
}

// Centipawns either side of the previous depth's score that
// search_root_with_aspiration tries first — half a pawn. Narrow enough to
// meaningfully cut the tree down versus a wide-open window, wide enough
// that an ordinary quiet move's natural score fluctuation between one
// depth and the next usually still lands inside it.
const ASPIRATION_WINDOW: i32 = 50;

/// One iterative-deepening depth's root search, with an aspiration
/// window: every depth past the first already has a real guess at the
/// answer (`previous_score`, the previous depth's result), so this tries
/// a narrow window around that guess first — most of the time the real
/// score still lands inside it, and alpha-beta prunes far more of the
/// tree with a narrow window than a wide-open one. If the guess turns out
/// wrong (the narrow search fails low or high — the true score lies
/// outside the window it was given, so the result can't be trusted as
/// exact), this falls back to a full-width re-search of the same depth
/// rather than trying to widen incrementally: simpler, and the guess
/// being wrong is the uncommon case, so paying full price for it when it
/// happens still comes out ahead of never trying the shortcut at all.
/// `use_aspiration` is `false` for the very first depth, which has no
/// previous score to aim at.
fn search_root_with_aspiration(
    game: &Game,
    moves: &[Move],
    depth: i32,
    cancel: &Cancellation,
    tt: &TranspositionTable,
    killers: &KillerMoves,
    previous_score: i32,
    use_aspiration: bool,
) -> (i32, Move) {
    if use_aspiration {
        let alpha = previous_score.saturating_sub(ASPIRATION_WINDOW);
        let beta = previous_score.saturating_add(ASPIRATION_WINDOW);
        let (score, mv) = best_root_score(game, moves, depth, cancel, tt, killers, alpha, beta);
        if cancel.is_cancelled() || (score > alpha && score < beta) {
            return (score, mv);
        }
        // Fail low or fail high — fall through to the full-width re-search below.
    }
    best_root_score(game, moves, depth, cancel, tt, killers, -(MATE_VALUE + 1), MATE_VALUE + 1)
}

/// Searches every root move in parallel, one thread per available CPU core
/// (fewer if there are fewer moves than that), and returns both the best
/// score and which move achieved it — the move is what the iterative
/// deepening loop in `search_cancellable` feeds forward as the next,
/// deeper pass's move-ordering preference. Each thread just needs the best
/// (score, move) among the moves it was handed — alpha-beta cutoffs still
/// happen normally *within* each thread's own subtree, they just aren't
/// shared *across* threads (nor is `alpha` narrowed across sibling root
/// moves within one call, unlike a single-threaded root would do — both
/// are a known, deliberate simplification from parallelizing at the
/// root), which is what makes this safe to parallelize without
/// synchronization beyond collecting the results at the end. `alpha`/
/// `beta` come from the caller (see `search_root_with_aspiration`) rather
/// than always being the widest possible window.
fn best_root_score(
    game: &Game,
    moves: &[Move],
    depth: i32,
    cancel: &Cancellation,
    tt: &TranspositionTable,
    killers: &KillerMoves,
    alpha: i32,
    beta: i32,
) -> (i32, Move) {
    let thread_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        // Using only *one* fewer core than available (the previous rule
        // here) still starves everything else under WSLg specifically:
        // its software rendering path (no real GPU is available inside
        // WSL, so it falls back to llvmpipe) is itself heavily
        // multi-threaded and competing for the same cores as the search.
        // A third of the cores, capped at a small absolute number, leaves
        // genuine headroom for the OS, the webview/compositor, and Tauri's
        // IPC dispatch regardless of how large this machine's core count
        // is — deeper search from here on should come from better pruning
        // rather than from claiming more threads, which just reopens this
        // exact problem. (The transposition table below is exactly that —
        // shared read/write access across these threads is also why it
        // needs its own internal locking rather than being handed out as
        // a plain mutable reference.)
        .div_ceil(3)
        .max(1)
        .min(6)
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
                            // Checked before each root move rather than
                            // inside negamax's recursion — cheap (one
                            // relaxed atomic load) and catches the common
                            // case (the position moved on while several
                            // root moves were still left to explore)
                            // without threading a check through every
                            // recursive call.
                            if cancel.is_cancelled() {
                                return None;
                            }
                            let mut next = game.clone();
                            next.make_move(mv).ok()?;
                            let score = -negamax(&next, depth - 1, 1, -beta, -alpha, tt, killers);
                            Some((score, mv))
                        })
                        .max_by_key(|(score, _)| *score)
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|handle| handle.join().expect("search thread panicked"))
            .max_by_key(|(score, _)| *score)
            // Only reachable if every thread's chunk was cancelled before
            // completing a single move — the caller (search_cancellable)
            // already discards a result found alongside a cancellation, so
            // this fallback's move is never actually shown to anyone.
            .unwrap_or((-(MATE_VALUE + 1), moves[0]))
    })
}

/// Returns a score from the perspective of whoever is to move in `game` —
/// positive is good for the mover, regardless of color.
fn negamax(
    game: &Game,
    depth: i32,
    ply: i32,
    mut alpha: i32,
    beta: i32,
    tt: &TranspositionTable,
    killers: &KillerMoves,
) -> i32 {
    let hash = game.zobrist_hash();
    let original_alpha = alpha;
    let (tt_score, tt_move) = tt.probe(hash, depth, alpha, beta);
    if let Some(score) = tt_score {
        return score;
    }

    let moves = order_moves(game, game.legal_moves_for_turn(), tt_move, killers.get(ply));

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
        return quiescence(game, alpha, beta);
    }

    // Null-move pruning: ask "if the side to move got to pass — handing the
    // opponent a free move — would I still be at least this good?" via a
    // cheap, deeply-reduced search. If even that best case for the
    // opponent still fails high, the real move (which is never worse than
    // passing) almost certainly would too, so the whole subtree gets
    // skipped without expanding it for real. `NULL_MOVE_REDUCTION` (how
    // many plies shallower that verification search goes) follows the
    // common R=2 choice from chess programming literature rather than
    // anything tuned against this engine specifically.
    //
    // Two guards keep this from misfiring: skipped while in check (a
    // "free move" that leaves the king hanging isn't a legal position to
    // reason about at all), and skipped whenever the mover has nothing but
    // king and pawns left (king-and-pawn endgames are exactly where
    // zugzwang — every move only makes your position worse — is common,
    // which is the one case "passing would be fine" does NOT imply "moving
    // is fine too").
    const NULL_MOVE_REDUCTION: i32 = 2;
    if depth > NULL_MOVE_REDUCTION
        && !rules::is_in_check(game.board(), game.turn())
        && has_non_pawn_material(game, game.turn())
    {
        let null_game = game.null_move();
        let score = -negamax(
            &null_game,
            depth - 1 - NULL_MOVE_REDUCTION,
            ply + 1,
            -beta,
            -beta + 1,
            tt,
            killers,
        );
        if score >= beta {
            return beta;
        }
    }

    // Principal variation search: move ordering (TT move, then captures,
    // then killers — see `order_moves`) means the first move tried here
    // is usually already the best one, so every move after it only needs
    // to answer a cheap yes/no question first — "is this better than
    // `alpha`?" — via a zero-width (null) window, rather than paying for
    // a full-width search to find out its exact value up front. Only a
    // move that actually answers "yes" (score lands strictly inside
    // (alpha, beta), meaning the null window wasn't enough to place it)
    // gets the full-width re-search its real value needs. When move
    // ordering is good, this is almost pure savings; when a later move
    // does turn out to beat every one before it, the one extra re-search
    // it costs is still cheaper than every sibling having paid full price
    // from the start.
    let mut best = -(MATE_VALUE + 1);
    let mut best_move = moves[0];
    let mut is_first_move = true;
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
        let score = if is_first_move {
            -negamax(&next, depth - 1, ply + 1, -beta, -alpha, tt, killers)
        } else {
            let scout = -negamax(&next, depth - 1, ply + 1, -alpha - 1, -alpha, tt, killers);
            if scout > alpha && scout < beta {
                -negamax(&next, depth - 1, ply + 1, -beta, -alpha, tt, killers)
            } else {
                scout
            }
        };
        is_first_move = false;
        if score > best {
            best = score;
            best_move = mv;
        }
        if best > alpha {
            alpha = best;
        }
        if alpha >= beta {
            // A quiet move (one that wasn't already going to be tried
            // early via the capture-ordering heuristic) just proved good
            // enough to cut this node off entirely — worth remembering
            // for sibling nodes at the same ply. Captures are excluded
            // since `order_moves` already puts them first on their own
            // merits; recording one here would just waste a killer slot
            // on a move that didn't need the help.
            if game.board().get(mv.to).is_none() {
                killers.record(ply, mv);
            }
            break; // alpha-beta cutoff — the opponent already has a better option elsewhere
        }
    }

    // Which kind of bound this is depends on *why* the loop stopped: cut
    // off early (beta reached) means the true score is at least `best`
    // (Lower) — the moves that would have proven an exact value were never
    // tried; ran every move without ever exceeding the window it started
    // with (`original_alpha`) means the true score is at most `best`
    // (Upper); anything in between actually is the position's true value
    // (Exact). See `Bound`'s own doc comment for how `probe` uses each one.
    let bound = if best >= beta {
        Bound::Lower
    } else if best <= original_alpha {
        Bound::Upper
    } else {
        Bound::Exact
    };
    tt.store(hash, depth, best, bound, Some(best_move));

    best
}

/// Keeps resolving captures past the nominal depth limit until the position
/// is "quiet" (no captures left to consider), rather than stopping cold the
/// instant `negamax` hits depth 0. Without this, the static eval at a leaf
/// can land mid-exchange — e.g. "I just won a pawn with my queen" one ply
/// before the obvious recapture — which is exactly the kind of blunder a
/// depth-limited search is prone to (the classic "horizon effect") and
/// exactly what would make the eval bar's number untrustworthy right at
/// the depths this app actually uses.
///
/// "Stand pat" (the mover may always choose not to capture at all, since
/// captures aren't forced in chess) bounds this recursion: every recursive
/// call must play an actual capture, which strictly reduces the number of
/// pieces left on the board, so this terminates on its own — no explicit
/// depth cap is needed. Not extended to check evasions when the mover is in
/// check at a leaf; that's a known, deliberate simplification for this
/// first pass, not an oversight.
fn quiescence(game: &Game, mut alpha: i32, beta: i32) -> i32 {
    let stand_pat = mover_relative_eval(game);
    if stand_pat >= beta {
        return beta;
    }
    if stand_pat > alpha {
        alpha = stand_pat;
    }

    let captures = order_moves(game, game.legal_moves_for_turn(), None, [None, None])
        .into_iter()
        .filter(|mv| game.board().get(mv.to).is_some());

    for mv in captures {
        let mut next = game.clone();
        if next.make_move(mv).is_err() {
            continue;
        }
        let score = -quiescence(&next, -beta, -alpha);
        if score >= beta {
            return beta;
        }
        if score > alpha {
            alpha = score;
        }
    }
    alpha
}

/// Sorts `preferred` first if present (see the iterative deepening loop in
/// `search_cancellable`, which passes the previous — shallower — depth's
/// best move here), then captures, biggest first (an approximation of the
/// classic MVV-LVA heuristic without weighing the attacker, just the
/// victim), then `killers` (see `KillerMoves`) ahead of the remaining
/// quiet moves they otherwise leave unordered. None of these criteria
/// change what the search finds — alpha-beta still visits every node it
/// would have anyway for a given depth — but trying the most promising
/// move first means the cutoffs in `negamax` trigger much earlier, so a
/// large chunk of the tree never gets visited at all.
fn order_moves(game: &Game, mut moves: Vec<Move>, preferred: Option<Move>, killers: [Option<Move>; 2]) -> Vec<Move> {
    moves.sort_by_key(|mv| {
        let is_preferred = preferred == Some(*mv);
        let captured_value = game.board().get(mv.to).map(|p| piece_value(p.kind)).unwrap_or(0);
        let is_killer = captured_value == 0 && killers.contains(&Some(*mv));
        (
            std::cmp::Reverse(is_preferred),
            std::cmp::Reverse(captured_value),
            std::cmp::Reverse(is_killer),
        )
    });
    moves
}

/// Whether `color` has any piece besides its king and pawns — see the
/// zugzwang guard on null-move pruning in `negamax`.
fn has_non_pawn_material(game: &Game, color: Color) -> bool {
    for index in 0..64 {
        let square = Square::from_index(index);
        if let Some(piece) = game.board().get(square) {
            if piece.color == color && piece.kind != PieceKind::Pawn && piece.kind != PieceKind::King {
                return true;
            }
        }
    }
    false
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
