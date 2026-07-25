use chesstnut::engine::board::{Board, Color, Piece, PieceKind, Square};
use chesstnut::engine::game::Game;
use chesstnut::engine::moves::Move;

fn place(board: &mut Board, square: Square, kind: PieceKind, color: Color) {
    board.set(square, Some(Piece { kind, color }));
}

#[test]
fn starting_position_matches_standard_fen() {
    let game = Game::new();
    assert_eq!(
        game.to_fen(),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    );
}

#[test]
fn pawn_double_step_sets_en_passant_target_and_switches_turn() {
    let mut game = Game::new();
    game.make_move(Move::new(Square::new(4, 1), Square::new(4, 3)))
        .unwrap(); // e2-e4

    assert_eq!(
        game.to_fen(),
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
    );
}

#[test]
fn halfmove_clock_and_fullmove_number_progress_correctly() {
    let mut game = Game::new();
    game.make_move(Move::new(Square::new(6, 0), Square::new(5, 2)))
        .unwrap(); // Nf3
    game.make_move(Move::new(Square::new(6, 7), Square::new(5, 5)))
        .unwrap(); // Nf6

    let fen = game.to_fen();
    let parts: Vec<&str> = fen.split(' ').collect();
    assert_eq!(parts[4], "2"); // two non-pawn, non-capture half-moves
    assert_eq!(parts[5], "2"); // White's 2nd move is next
}

#[test]
fn castling_rights_reflected_in_fen_after_rook_moves() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(7, 0), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    let mut game = Game::from_board(board, Color::White);

    game.make_move(Move::new(Square::new(7, 0), Square::new(7, 3)))
        .unwrap();

    let castling_field = game.to_fen().split(' ').nth(2).unwrap().to_string();
    assert!(!castling_field.contains('K'));
}

// ---------- import ----------

#[test]
fn import_then_export_round_trips_a_custom_position() {
    let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
    let game = Game::from_fen(fen).unwrap();
    assert_eq!(game.to_fen(), fen);
}

#[test]
fn imported_position_is_playable() {
    let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
    let mut game = Game::from_fen(fen).unwrap();

    let result = game.make_move(Move::new(Square::new(4, 6), Square::new(4, 4))); // e7-e5
    assert!(result.is_ok());
    assert_eq!(game.turn(), Color::White);
}

#[test]
fn imported_castling_rights_restrict_legal_castling() {
    let fen = "4k3/8/8/8/8/8/8/4K2R w K - 0 1";
    let game = Game::from_fen(fen).unwrap();

    let moves = game.legal_moves_from(Square::new(4, 0)); // king on e1
    assert!(moves.contains(&Move::new(Square::new(4, 0), Square::new(6, 0)))); // O-O available
    assert!(!moves.contains(&Move::new(Square::new(4, 0), Square::new(2, 0)))); // O-O-O not
}

#[test]
fn malformed_fen_wrong_field_count_is_rejected() {
    let result = Game::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -");
    assert!(result.is_err());
}

#[test]
fn malformed_fen_rank_not_summing_to_eight_is_rejected() {
    let result = Game::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBN w KQkq - 0 1");
    assert!(result.is_err());
}

#[test]
fn malformed_fen_bad_piece_character_is_rejected() {
    let result = Game::from_fen("znbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    assert!(result.is_err());
}
