use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub fn opponent(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub kind: PieceKind,
    pub color: Color,
}

/// A square on the board, addressed by 0-based file (a=0..h=7) and rank (1=0..8=7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Square {
    pub file: u8,
    pub rank: u8,
}

impl Square {
    pub fn new(file: u8, rank: u8) -> Self {
        Square { file, rank }
    }

    pub fn to_index(self) -> usize {
        (self.rank as usize) * 8 + self.file as usize
    }

    pub fn from_index(index: usize) -> Self {
        Square {
            file: (index % 8) as u8,
            rank: (index / 8) as u8,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Board {
    squares: [Option<Piece>; 64],
}

impl Board {
    pub fn empty() -> Self {
        Board { squares: [None; 64] }
    }

    pub fn starting_position() -> Self {
        let mut board = Board::empty();

        for file in 0..8 {
            board.set(
                Square::new(file, 1),
                Some(Piece {
                    kind: PieceKind::Pawn,
                    color: Color::White,
                }),
            );
            board.set(
                Square::new(file, 6),
                Some(Piece {
                    kind: PieceKind::Pawn,
                    color: Color::Black,
                }),
            );
        }

        let back_rank = [
            PieceKind::Rook,
            PieceKind::Knight,
            PieceKind::Bishop,
            PieceKind::Queen,
            PieceKind::King,
            PieceKind::Bishop,
            PieceKind::Knight,
            PieceKind::Rook,
        ];

        for (file, kind) in back_rank.into_iter().enumerate() {
            board.set(
                Square::new(file as u8, 0),
                Some(Piece {
                    kind,
                    color: Color::White,
                }),
            );
            board.set(
                Square::new(file as u8, 7),
                Some(Piece {
                    kind,
                    color: Color::Black,
                }),
            );
        }

        board
    }

    pub fn get(&self, square: Square) -> Option<Piece> {
        self.squares[square.to_index()]
    }

    pub fn set(&mut self, square: Square, piece: Option<Piece>) {
        self.squares[square.to_index()] = piece;
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in (0..8).rev() {
            for file in 0..8 {
                let symbol = match self.get(Square::new(file, rank)) {
                    Some(piece) => piece_symbol(piece),
                    None => '.',
                };
                write!(f, "{symbol} ")?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

pub(crate) fn piece_symbol(piece: Piece) -> char {
    let letter = match piece.kind {
        PieceKind::Pawn => 'p',
        PieceKind::Knight => 'n',
        PieceKind::Bishop => 'b',
        PieceKind::Rook => 'r',
        PieceKind::Queen => 'q',
        PieceKind::King => 'k',
    };
    match piece.color {
        Color::White => letter.to_ascii_uppercase(),
        Color::Black => letter,
    }
}

