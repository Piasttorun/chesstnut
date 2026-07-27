use chesstnut::ai::{search, Score};
use chesstnut::engine::game::Game;

/// The classic king-and-pawn "opposition" position: White King e6, Pawn e5,
/// Black King e8 — the textbook example of reciprocal zugzwang. Whoever is
/// NOT forced to move wins the opposition: with Black to move, Black must
/// abandon e8 (White's king escorts the pawn home); with White to move,
/// White must step aside first and Black retakes the opposition, holding
/// the draw. Both sides have only a king (Black) or a king and a pawn
/// (White) — exactly the material shape that makes null-move pruning's
/// "if I could pass, would I still be fine?" question unsound (see the
/// zugzwang guard in `ai::search::negamax`): passing would genuinely be
/// fine here, but being forced to move is what loses.
///
/// This is a regression test for that guard, not a general search test —
/// confirmed (see the commit that added this file) that removing the
/// guard changes depth 8's Black-to-move score from the correct
/// `Centipawns(815)` (search finds the forced promotion) down to a wrong,
/// drawish `Centipawns(80)` (null-move pruning wrongly convinces itself
/// Black is fine, hiding the forced loss). Depth 8 specifically is used
/// because it's the shallowest depth at which that divergence appears —
/// shallower depths don't yet see far enough to reach the promotion
/// either way, guarded or not, so they wouldn't catch a regression here.
#[test]
fn null_move_pruning_does_not_hide_pawn_endgame_zugzwang() {
    let black_to_move =
        Game::from_fen("4k3/8/4K3/4P3/8/8/8/8 b - - 0 1").expect("valid FEN");

    assert_eq!(search(&black_to_move, 8), Score::Centipawns(815));
}
