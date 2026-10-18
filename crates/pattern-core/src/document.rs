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

/// Cubic curve connectivity; coordinates live only in the piece's point table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CubicBezierSegment {
    pub start: PointId,
    pub control_start: PointId,
    pub control_end: PointId,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub beziers: Vec<CubicBezierSegment>,
    pub grainline: Grainline,
}

impl Piece {
    pub fn point(&self, id: PointId) -> Option<&PatternPoint> {
        self.points.iter().find(|p| p.id == id)
    }

    pub fn bezier_controls(&self, curve: &CubicBezierSegment) -> Option<[MmPoint; 4]> {
        Some([
            self.point(curve.start)?.position,
            self.point(curve.control_start)?.position,
            self.point(curve.control_end)?.position,
            self.point(curve.end)?.position,
        ])
    }

    pub fn is_bezier_handle(&self, point: PointId) -> bool {
        self.beziers
            .iter()
            .any(|curve| curve.control_start == point || curve.control_end == point)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub schema_version: u32,
    pub pieces: Vec<Piece>,
}

impl Document {
    pub const SCHEMA_VERSION: u32 = 2;

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
                beziers: Vec::new(),
                grainline: Grainline {
                    start: MmPoint::new(200.0, 120.0),
                    end: MmPoint::new(200.0, 480.0),
                },
            }],
        }
    }

    /// Sample panel with an inward-curved top and two independently editable handles.
    pub fn sample_curved_skirt() -> Self {
        let mut document = Self::sample_skirt();
        let piece = &mut document.pieces[0];
        piece.metadata.notes = "Prototype curved panel; no seam allowance".into();
        piece.segments.remove(2);
        piece.points.extend([
            PatternPoint {
                id: PointId(5),
                position: MmPoint::new(300.0, 520.0),
            },
            PatternPoint {
                id: PointId(6),
                position: MmPoint::new(100.0, 520.0),
            },
        ]);
        piece.beziers.push(CubicBezierSegment {
            start: PointId(3),
            control_start: PointId(5),
            control_end: PointId(6),
            end: PointId(4),
        });
        document
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
            for curve in &piece.beziers {
                let ids = [
                    curve.start,
                    curve.control_start,
                    curve.control_end,
                    curve.end,
                ];
                if ids.into_iter().collect::<BTreeSet<_>>().len() != 4
                    || ids.iter().any(|id| !point_ids.contains(id))
                {
                    return Err(invalid("Bezier requires four distinct existing point IDs"));
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
        let mut document: Self = serde_json::from_str(json)?;
        // Version 1 contains only straight segments. Normalize it at the load boundary.
        if document.schema_version == 1 && document.pieces.iter().all(|p| p.beziers.is_empty()) {
            document.schema_version = Self::SCHEMA_VERSION;
        }
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
            .replace("\"schema_version\": 2", "\"schema_version\": 99");
        assert!(Document::from_json(&json).is_err());
        let json = Document::sample_skirt()
            .to_json()
            .unwrap()
            .replace("\"end\": 2", "\"end\": 999");
        assert!(Document::from_json(&json).is_err());
    }

    #[test]
    fn curved_document_round_trips_references_and_rejects_invalid_controls() {
        let document = Document::sample_curved_skirt();
        let json = document.to_json().unwrap();
        assert_eq!(Document::from_json(&json).unwrap(), document);
        assert_eq!(Document::from_json(&json).unwrap().to_json().unwrap(), json);
        let piece = &document.pieces[0];
        let controls = piece.bezier_controls(&piece.beziers[0]).unwrap();
        assert_eq!(
            crate::cubic_bezier_point(controls, 0.5).unwrap(),
            MmPoint::new(200.0, 540.0)
        );
        for invalid_id in [PointId(999), PointId(3), PointId(6)] {
            let mut invalid = document.clone();
            invalid.pieces[0].beziers[0].control_start = invalid_id;
            assert!(invalid.validate().is_err());
            assert!(invalid.to_json().is_err());
            let json = serde_json::to_string(&invalid).unwrap();
            assert!(Document::from_json(&json).is_err());
        }
        let mut invalid = document;
        invalid.pieces[0].points[4].position.x = f64::INFINITY;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn version_one_loads_as_version_two_but_cannot_claim_curve_support() {
        let current = Document::sample_skirt();
        let old_json = current
            .to_json()
            .unwrap()
            .replace("\"schema_version\": 2", "\"schema_version\": 1");
        assert!(!old_json.contains("beziers"));
        assert_eq!(Document::from_json(&old_json).unwrap(), current);
        let invalid_version = Document::sample_curved_skirt()
            .to_json()
            .unwrap()
            .replace("\"schema_version\": 2", "\"schema_version\": 1");
        assert!(Document::from_json(&invalid_version).is_err());
    }
}
