//! Deterministic millimeter geometry and editing; no GUI or filesystem dependencies.

mod command;
mod document;
mod geometry;

pub use command::PatternCommand;
pub use document::{
    Document, Grainline, LineSegment, PatternPoint, Piece, PieceId, PieceMetadata, PointId,
};
pub use geometry::{CanvasPoint, MmPoint, ViewTransform};

#[derive(Debug)]
pub enum PatternError {
    Invalid(String),
    PieceNotFound(PieceId),
    PointNotFound { piece: PieceId, point: PointId },
    Json(serde_json::Error),
}

impl std::fmt::Display for PatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "{message}"),
            Self::PieceNotFound(id) => write!(f, "Piece {} not found", id.0),
            Self::PointNotFound { piece, point } => {
                write!(f, "Point {} not found in piece {}", point.0, piece.0)
            }
            Self::Json(error) => write!(f, "JSON: {error}"),
        }
    }
}

impl std::error::Error for PatternError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for PatternError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
