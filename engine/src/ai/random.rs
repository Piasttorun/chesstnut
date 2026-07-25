use rand::seq::SliceRandom;

use crate::engine::game::Game;
use crate::engine::moves::Move;

use super::Engine;

/// The simplest possible opponent: picks uniformly at random among every
/// legal move, with no evaluation of the resulting position at all.
pub struct RandomEngine;

impl Engine for RandomEngine {
    fn choose_move(&self, game: &Game) -> Option<Move> {
        game.legal_moves_for_turn()
            .choose(&mut rand::thread_rng())
            .copied()
    }
}
