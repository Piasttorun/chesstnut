//! Roadmap for chesstnut's AI opponent, staged from simplest to most
//! involved:
//!
//! 1. **Scaffolding (this stage)** — an `Engine` trait plus a
//!    [`RandomEngine`] that picks uniformly among legal moves. It doesn't
//!    evaluate positions at all; it exists so the "play against AI" flow
//!    (color choice, whose turn triggers an engine call, waiting on a move)
//!    can be built and tested end-to-end before any real search exists.
//!    `RandomEngine` isn't just a throwaway stub, either — it's a
//!    legitimate, permanently-kept "easy" difficulty once harder engines
//!    exist alongside it.
//! 2. **Score bar** — wire up [`Score`] through the Tauri layer so the UI
//!    can show an evaluation ("+4", "mate in 5") independently of whether
//!    anything is actually searching for a best move yet.
//! 3. **Real search** — a classical depth-limited minimax/alpha-beta
//!    engine scored by [`Score`], the kind of design chess engines have
//!    used since the 1980s. Pure CPU work: alpha-beta pruning is
//!    sequential and branch-heavy by nature and doesn't parallelize onto a
//!    GPU in any way that helps — that's why even modern engines like
//!    Stockfish stay CPU-bound.
//! 4. **(Speculative, much later)** GPU acceleration only becomes relevant
//!    if evaluation moves to a neural network (AlphaZero/Leela Chess Zero
//!    style) — a different architecture from stage 3's search, not an
//!    optimization of it. Not planned unless a later stage actually wants
//!    NN-based evaluation.

mod eval;
mod random;

pub use eval::evaluate;
pub use random::RandomEngine;

use crate::engine::game::Game;
use crate::engine::moves::Move;

/// A chess opponent: given the current position, picks a move (or `None`
/// if there isn't one — checkmate/stalemate/game already over). Kept as a
/// trait so [`RandomEngine`] today and a real search engine in stage 3 are
/// interchangeable from the caller's side.
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
