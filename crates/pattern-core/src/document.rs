use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{MmPoint, PatternError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PieceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PointId(pub u64);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternPoint {
    pub id: PointId,
    pub position: MmPoint,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineSegment {
    pub start: PointId,
    pub end: PointId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceMetadata {
    pub name: String,
    pub cut_quantity: u32,
    pub fabric: String,
    pub notes: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grainline {
    pub start: MmPoint,
    pub end: MmPoint,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Piece {
    pub id: PieceId,
    pub metadata: PieceMetadata,
    pub points: Vec<PatternPoint>,
    pub segments: Vec<LineSegment>,
    pub grainline: Grainline,
}

impl Piece {
    pub fn point(&self, id: PointId) -> Option<&PatternPoint> {
        self.points.iter().find(|p| p.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub schema_version: u32,
    pub pieces: Vec<Piece>,
}

impl Document {
    pub const SCHEMA_VERSION: u32 = 1;

    /// Independently authored rectangular skirt panel: 400 mm wide, 600 mm long.
    pub fn sample_skirt() -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            pieces: vec![Piece {
                id: PieceId(1),
                metadata: PieceMetadata {
                    name: "Skirt panel".into(),
                    cut_quantity: 2,
                    fabric: "Woven".into(),
                    notes: "Prototype rectangle; no seam allowance".into(),
                },
                points: [
                    (1, 0.0, 0.0),
                    (2, 400.0, 0.0),
                    (3, 400.0, 600.0),
                    (4, 0.0, 600.0),
                ]
                .into_iter()
                .map(|(id, x, y)| PatternPoint {
                    id: PointId(id),
                    position: MmPoint::new(x, y),
                })
                .collect(),
                segments: [(1, 2), (2, 3), (3, 4), (4, 1)]
                    .into_iter()
                    .map(|(start, end)| LineSegment {
                        start: PointId(start),
                        end: PointId(end),
                    })
                    .collect(),
                grainline: Grainline {
                    start: MmPoint::new(200.0, 120.0),
                    end: MmPoint::new(200.0, 480.0),
                },
            }],
        }
    }

    pub fn piece(&self, id: PieceId) -> Option<&Piece> {
        self.pieces.iter().find(|p| p.id == id)
    }

    pub fn validate(&self) -> Result<(), PatternError> {
        let invalid = |message: &str| PatternError::Invalid(message.into());
        if self.schema_version != Self::SCHEMA_VERSION {
            return Err(invalid("Unsupported document schema version"));
        }
        let mut piece_ids = BTreeSet::new();
        for piece in &self.pieces {
            if !piece_ids.insert(piece.id) {
                return Err(invalid("Duplicate piece ID"));
            }
            if piece.metadata.name.trim().is_empty() || piece.metadata.cut_quantity == 0 {
                return Err(invalid("Piece needs a name and positive cut quantity"));
            }
            let mut point_ids = BTreeSet::new();
            for point in &piece.points {
                if !point_ids.insert(point.id) {
                    return Err(invalid("Duplicate point ID within piece"));
                }
                if !point.position.is_finite() {
                    return Err(invalid("Point coordinates must be finite millimeters"));
                }
            }
            for segment in &piece.segments {
                if segment.start == segment.end
                    || !point_ids.contains(&segment.start)
                    || !point_ids.contains(&segment.end)
                {
                    return Err(invalid("Segment requires two distinct existing point IDs"));
                }
            }
            let grain = &piece.grainline;
            if !grain.start.is_finite() || !grain.end.is_finite() || grain.start == grain.end {
                return Err(invalid("Grainline requires finite, distinct endpoints"));
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, PatternError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self, PatternError> {
        let document: Self = serde_json::from_str(json)?;
        document.validate()?;
        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_is_physical_and_json_round_trips_exactly() {
        let mut doc = Document::sample_skirt();
        doc.pieces[0].points[0].position = MmPoint::new(-12.345678901234567, 0.12345678901234568);
        let json = doc.to_json().unwrap();
        assert_eq!(Document::from_json(&json).unwrap(), doc);
        assert_eq!(Document::from_json(&json).unwrap().to_json().unwrap(), json);
        assert_eq!(doc.pieces[0].points[2].position, MmPoint::new(400.0, 600.0));
    }

    #[test]
    fn rejects_invalid_document_structure() {
        let base = Document::sample_skirt();
        let mut cases = Vec::new();
        let mut doc = base.clone();
        doc.schema_version = 99;
        cases.push(doc);
        let mut doc = base.clone();
        doc.pieces.push(doc.pieces[0].clone());
        cases.push(doc);
        let mut doc = base.clone();
        doc.pieces[0].points[1].id = PointId(1);
        cases.push(doc);
        let mut doc = base.clone();
        doc.pieces[0].segments[0].end = PointId(999);
        cases.push(doc);
        let mut doc = base.clone();
        doc.pieces[0].segments[0].end = PointId(1);
        cases.push(doc);
        let mut doc = base.clone();
        doc.pieces[0].grainline.end = doc.pieces[0].grainline.start;
        cases.push(doc);
        let mut doc = base.clone();
        doc.pieces[0].points[0].position.x = f64::NAN;
        cases.push(doc);
        let mut doc = base.clone();
        doc.pieces[0].metadata.cut_quantity = 0;
        cases.push(doc);
        let mut doc = base.clone();
        doc.pieces[0].metadata.name.clear();
        cases.push(doc);
        for doc in cases {
            assert!(doc.validate().is_err());
            assert!(doc.to_json().is_err());
        }
    }

    #[test]
    fn json_rejects_malformed_unknown_and_invalid_data() {
        assert!(Document::from_json("{broken").is_err());
        assert!(
            Document::from_json(r#"{"schema_version":1,"pieces":[],"units":"inches"}"#).is_err()
        );
        let json = Document::sample_skirt()
            .to_json()
            .unwrap()
            .replace("\"schema_version\": 1", "\"schema_version\": 99");
        assert!(Document::from_json(&json).is_err());
        let json = Document::sample_skirt()
            .to_json()
            .unwrap()
            .replace("\"end\": 2", "\"end\": 999");
        assert!(Document::from_json(&json).is_err());
    }
}
