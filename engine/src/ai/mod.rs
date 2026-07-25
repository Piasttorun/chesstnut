//! Roadmap for chesstnut's AI opponent, staged from simplest to most
//! involved:
//!
//! 1. **Scaffolding (done)** — an `Engine` trait plus a [`RandomEngine`]
//!    that picks uniformly among legal moves. `RandomEngine` isn't just a
//!    throwaway stub, either — it's a legitimate, permanently-kept "easy"
//!    difficulty once harder engines exist alongside it.
//! 2. **Score bar (done)** — [`Score`] wired through the Tauri layer so the
//!    UI can show an evaluation ("+4", "mate in 5") independently of
//!    whether anything is actually playing moves yet.
//! 3. **Real search (done)** — a classical depth-limited minimax/alpha-beta
//!    engine ([`search`]/[`SearchEngine`]) scored by [`Score`], with
//!    iterative deepening, quiescence search at the leaves, and root-level
//!    parallelism. Pure CPU work: alpha-beta pruning is sequential and
//!    branch-heavy by nature and doesn't parallelize onto a GPU in any way
//!    that helps — that's why even modern engines like Stockfish stay
//!    CPU-bound. Next lever here is a transposition table, not more
//!    threads.
//! 4. **Play vs AI (next)** — wire an [`Engine`] (starting with
//!    [`SearchEngine`]) into the actual game loop so it plays a side
//!    instead of only being consulted for the eval bar. Once there's more
//!    than one real [`Engine`] worth choosing between (this search vs.
//!    [`RandomEngine`], and later NNUE / external UCI engines / an
//!    LLM-backed one), that becomes a settings choice rather than a
//!    hardcoded one.
//! 5. **(Speculative, much later)** GPU acceleration only becomes relevant
//!    if evaluation moves to a neural network (AlphaZero/Leela Chess Zero
//!    style) — a different architecture from stage 3's search, not an
//!    optimization of it. Not planned unless a later stage actually wants
//!    NN-based evaluation.

mod eval;
mod opening_book;
mod random;
mod search;

pub use eval::evaluate;
pub use opening_book::book_move;
pub use random::RandomEngine;
pub use search::{best_move, best_move_cancellable, search, search_cancellable, Cancellation, SearchEngine};

use crate::engine::game::Game;
use crate::engine::moves::Move;

/// A chess opponent: given the current position, picks a move (or `None`
/// if there isn't one — checkmate/stalemate/game already over). Kept as a
/// trait so [`RandomEngine`] and [`SearchEngine`] are interchangeable from
/// the caller's side, and so a future NNUE or external-process engine can
/// join them without changing whatever calls `choose_move`.
pub trait Engine {
    fn choose_move(&self, game: &Game) -> Option<Move>;
}

/// A position evaluation, from the mover's perspective. `Centipawns` is the
/// standard chess-engine unit (100 = one pawn's worth of advantage);
/// `MateIn` counts plies to a forced mate. Nothing produces one of these
/// yet — it exists so stage 2 (the score bar) and stage 3 (real search)
/// have a shared type to build against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Score {
    Centipawns(i32),
    MateIn(i32),
}
