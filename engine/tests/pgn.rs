use chesstnut::engine::board::{Board, Color, Piece, PieceKind, Square};
use chesstnut::engine::game::Game;
use chesstnut::engine::moves::Move;

fn place(board: &mut Board, square: Square, kind: PieceKind, color: Color) {
    board.set(square, Some(Piece { kind, color }));
}

#[test]
fn move_history_records_pawn_and_knight_moves_in_san() {
    let mut game = Game::new();
    game.make_move(Move::new(Square::new(4, 1), Square::new(4, 3)))
        .unwrap(); // e4
    game.make_move(Move::new(Square::new(4, 6), Square::new(4, 4)))
        .unwrap(); // e5
    game.make_move(Move::new(Square::new(6, 0), Square::new(5, 2)))
        .unwrap(); // Nf3

    assert_eq!(game.move_history(), &["e4", "e5", "Nf3"]);
}

#[test]
fn captures_use_x_notation() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    place(&mut board, Square::new(3, 3), PieceKind::Pawn, Color::White); // d4
    place(&mut board, Square::new(4, 4), PieceKind::Pawn, Color::Black); // e5
    let mut game = Game::from_board(board, Color::White);

    game.make_move(Move::new(Square::new(3, 3), Square::new(4, 4)))
        .unwrap(); // dxe5
    assert_eq!(game.move_history(), &["dxe5"]);
}

#[test]
fn castling_recorded_as_o_o() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(7, 0), PieceKind::Rook, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    let mut game = Game::from_board(board, Color::White);

    game.make_move(Move::new(Square::new(4, 0), Square::new(6, 0)))
        .unwrap();
    assert_eq!(game.move_history(), &["O-O"]);
}

#[test]
fn check_move_gets_plus_suffix() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black); // e8
    place(&mut board, Square::new(0, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(0, 3), PieceKind::Rook, Color::White); // a4
    let mut game = Game::from_board(board, Color::White);

    game.make_move(Move::new(Square::new(0, 3), Square::new(4, 3)))
        .unwrap(); // Ra4-e4+, king can still step off the e-file
    assert_eq!(game.move_history(), &["Re4+"]);
}

#[test]
fn checkmate_move_gets_hash_suffix() {
    let mut board = Board::empty();
    place(&mut board, Square::new(0, 7), PieceKind::King, Color::Black); // a8
    place(&mut board, Square::new(1, 5), PieceKind::King, Color::White); // b6
    place(&mut board, Square::new(0, 0), PieceKind::Queen, Color::White); // a1
    let mut game = Game::from_board(board, Color::White);

    game.make_move(Move::new(Square::new(0, 0), Square::new(0, 6)))
        .unwrap(); // Qa1-a7#
    assert_eq!(game.move_history(), &["Qa7#"]);
}

#[test]
fn ambiguous_knight_moves_are_disambiguated_by_file() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White);
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black);
    place(&mut board, Square::new(1, 3), PieceKind::Knight, Color::White); // b4
    place(&mut board, Square::new(5, 3), PieceKind::Knight, Color::White); // f4
    let mut game = Game::from_board(board, Color::White);

    // Both knights can legally reach d5.
    game.make_move(Move::new(Square::new(1, 3), Square::new(3, 4)))
        .unwrap();
    assert_eq!(game.move_history(), &["Nbd5"]);
}

#[test]
fn to_pgn_includes_headers_and_movetext() {
    let mut game = Game::new();
    game.make_move(Move::new(Square::new(4, 1), Square::new(4, 3)))
        .unwrap(); // e4
    game.make_move(Move::new(Square::new(4, 6), Square::new(4, 4)))
        .unwrap(); // e5

    let pgn = game.to_pgn();
    assert!(pgn.contains("[Event \"Casual Game\"]"));
    assert!(pgn.contains("1. e4 e5"));
}

#[test]
fn to_pgn_result_reflects_a_resignation() {
    let mut game = Game::new();
    game.make_move(Move::new(Square::new(4, 1), Square::new(4, 3)))
        .unwrap(); // e4, now Black to move

    game.resign().unwrap(); // Black resigns on its own turn -> White wins

    let pgn = game.to_pgn();
    assert!(pgn.contains("[Result \"1-0\"]"));
    assert!(pgn.trim_end().ends_with("1-0"));
}

#[test]
fn to_pgn_result_is_undecided_before_the_game_ends() {
    let game = Game::new();
    assert!(game.to_pgn().contains("[Result \"*\"]"));
}

#[test]
fn to_pgn_date_is_not_the_old_placeholder() {
    let game = Game::new();
    assert!(!game.to_pgn().contains("[Date \"????.??.??\"]"));
}

// ---------- import ----------

#[test]
fn import_pgn_replays_moves_in_order() {
    let game = Game::import_pgn("1. e4 e5 2. Nf3 Nc6 3. Bb5").unwrap();
    assert_eq!(game.move_history(), &["e4", "e5", "Nf3", "Nc6", "Bb5"]);
}

#[test]
fn import_pgn_ignores_headers_and_result_token() {
    let pgn = "[Event \"Test\"]\n[Site \"?\"]\n\n1. e4 e5 2. Nf3 Nc6 1-0";
    let game = Game::import_pgn(pgn).unwrap();
    assert_eq!(game.move_history(), &["e4", "e5", "Nf3", "Nc6"]);
}

#[test]
fn import_pgn_drops_a_trailing_variation() {
    // A real Lichess export: someone took back move 6 during analysis, and
    // the abandoned line (6. Qc4) is preserved as a variation rather than
    // dropped from the export.
    let pgn = "1. e4 e5 2. Nf3 Nc6 3. d4 exd4 4. Nxd4 Nxd4 5. Qxd4 Qf6 6. Qa4 (6. Qc4)";
    let game = Game::import_pgn(pgn).unwrap();
    assert_eq!(
        game.move_history(),
        &["e4", "e5", "Nf3", "Nc6", "d4", "exd4", "Nxd4", "Nxd4", "Qxd4", "Qf6", "Qa4"]
    );
}

#[test]
fn import_pgn_drops_a_nested_variation() {
    let pgn = "1. e4 e5 2. Nf3 (2. Bc4 (2. Nc3 Nf6) Nc6) Nc6 3. Bb5";
    let game = Game::import_pgn(pgn).unwrap();
    assert_eq!(game.move_history(), &["e4", "e5", "Nf3", "Nc6", "Bb5"]);
}

#[test]
fn import_pgn_drops_comments() {
    let pgn = "1. e4 { good move } e5 2. Nf3 { [%clk 0:05:00] } Nc6";
    let game = Game::import_pgn(pgn).unwrap();
    assert_eq!(game.move_history(), &["e4", "e5", "Nf3", "Nc6"]);
}

#[test]
fn import_pgn_rejects_a_token_that_matches_no_legal_move() {
    let result = Game::import_pgn("1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Zz9");
    assert!(result.is_err());
}

#[test]
fn export_then_import_round_trips_move_history() {
    let mut game = Game::new();
    game.make_move(Move::new(Square::new(4, 1), Square::new(4, 3)))
        .unwrap(); // e4
    game.make_move(Move::new(Square::new(4, 6), Square::new(4, 4)))
        .unwrap(); // e5
    game.make_move(Move::new(Square::new(6, 0), Square::new(5, 2)))
        .unwrap(); // Nf3

    let pgn = game.to_pgn();
    let replayed = Game::import_pgn(&pgn).unwrap();
    assert_eq!(replayed.move_history(), game.move_history());
}
