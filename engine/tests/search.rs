use chesstnut::ai::{search, Score};
use chesstnut::engine::board::{Board, Color, Piece, PieceKind, Square};
use chesstnut::engine::game::Game;

fn place(board: &mut Board, square: Square, kind: PieceKind, color: Color) {
    board.set(square, Some(Piece { kind, color }));
}

#[test]
fn search_finds_white_mate_in_one() {
    let mut board = Board::empty();
    place(&mut board, Square::new(0, 7), PieceKind::King, Color::Black); // a8
    place(&mut board, Square::new(1, 5), PieceKind::King, Color::White); // b6
    place(&mut board, Square::new(0, 0), PieceKind::Queen, Color::White); // a1
    let game = Game::from_board(board, Color::White);

    // Depth 2 (not just 1) confirms the search still finds and prefers the
    // immediate mate over merely searching one ply deeper without acting
    // on it.
    assert_eq!(search(&game, 2), Score::MateIn(1));
}

#[test]
fn search_finds_black_mate_in_one() {
    // Same shape as the White mate above, mirrored top-to-bottom with
    // colors swapped.
    let mut board = Board::empty();
    place(&mut board, Square::new(0, 0), PieceKind::King, Color::White); // a1
    place(&mut board, Square::new(1, 2), PieceKind::King, Color::Black); // b3
    place(&mut board, Square::new(0, 7), PieceKind::Queen, Color::Black); // a8
    let game = Game::from_board(board, Color::Black);

    assert_eq!(search(&game, 2), Score::MateIn(-1));
}

#[test]
fn search_returns_centipawns_in_a_quiet_position() {
    let mut board = Board::empty();
    place(&mut board, Square::new(4, 0), PieceKind::King, Color::White); // e1
    place(&mut board, Square::new(4, 7), PieceKind::King, Color::Black); // e8
    place(&mut board, Square::new(3, 0), PieceKind::Queen, Color::White); // d1
    let game = Game::from_board(board, Color::White);

    // No captures are available to either side, so a shallow search can't
    // find anything better or worse than the static material count.
    assert_eq!(search(&game, 1), Score::Centipawns(900));
}

#[test]
fn search_returns_zero_at_stalemate() {
    let mut board = Board::empty();
    place(&mut board, Square::new(7, 7), PieceKind::King, Color::Black); // h8
    place(&mut board, Square::new(5, 6), PieceKind::King, Color::White); // f7
    place(&mut board, Square::new(6, 5), PieceKind::Queen, Color::White); // g6
    let game = Game::from_board(board, Color::Black);

    assert_eq!(search(&game, 3), Score::Centipawns(0));
}

#[test]
fn search_does_not_panic_before_a_time_control_is_chosen() {
    // `legal_moves_for_turn` is a pure board check and doesn't know about
    // `awaiting_clock_choice`, so a naive search would hand `make_move`
    // moves it's guaranteed to reject. Regression test for a real crash:
    // checking the eval-bar box while still on the "choose a time control"
    // screen took the app down.
    let game = Game::new_pending_clock();
    assert_eq!(search(&game, 3), Score::Centipawns(0));
}
