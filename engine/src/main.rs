use chesstnut::engine::board::Board;

fn main() {
    let board = Board::starting_position();
    print!("{board}");
}
