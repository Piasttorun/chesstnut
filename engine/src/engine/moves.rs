use crate::engine::board::{PieceKind, Square};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub from: Square,
    pub to: Square,
    pub promotion: Option<PieceKind>,
}

impl Move {
    pub fn new(from: Square, to: Square) -> Self {
        Move {
            from,
            to,
            promotion: None,
        }
    }

    pub fn promotion(from: Square, to: Square, promotion: PieceKind) -> Self {
        Move {
            from,
            to,
            promotion: Some(promotion),
        }
    }

    pub fn is_promotion(self) -> bool {
        self.promotion.is_some()
    }
}

