use crate::{Document, MmPoint, PatternError, PieceId, PointId};
use serde::{Deserialize, Serialize};

/// Shared edit boundary. Positions and relative offsets are physical millimeters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PatternCommand {
    MovePoint {
        piece_id: PieceId,
        point_id: PointId,
        to: MmPoint,
    },
    TranslatePoint {
        piece_id: PieceId,
        point_id: PointId,
        delta: MmPoint,
    },
}

impl Document {
    /// Validate before mutation; every failure leaves the document unchanged.
    pub fn apply(&mut self, command: PatternCommand) -> Result<(), PatternError> {
        self.validate()?;
        let (piece_id, point_id, to) = match command {
            PatternCommand::MovePoint {
                piece_id,
                point_id,
                to,
            } => (piece_id, point_id, to),
            PatternCommand::TranslatePoint {
                piece_id,
                point_id,
                delta,
            } => {
                if !delta.is_finite() {
                    return Err(PatternError::Invalid(
                        "Translation offset must be finite millimeters".into(),
                    ));
                }
                let piece = self
                    .piece(piece_id)
                    .ok_or(PatternError::PieceNotFound(piece_id))?;
                let point = piece.point(point_id).ok_or(PatternError::PointNotFound {
                    piece: piece_id,
                    point: point_id,
                })?;
                (
                    piece_id,
                    point_id,
                    MmPoint::new(point.position.x + delta.x, point.position.y + delta.y),
                )
            }
        };
        // Check the computed target too: finite offsets can still overflow.
        if !to.is_finite() {
            return Err(PatternError::Invalid(
                "Move target must be finite millimeters".into(),
            ));
        }
        let piece = self
            .pieces
            .iter_mut()
            .find(|p| p.id == piece_id)
            .ok_or(PatternError::PieceNotFound(piece_id))?;
        let point = piece.points.iter_mut().find(|p| p.id == point_id).ok_or(
            PatternError::PointNotFound {
                piece: piece_id,
                point: point_id,
            },
        )?;
        point.position = to;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn move_to(piece: u64, point: u64, to: MmPoint) -> PatternCommand {
        PatternCommand::MovePoint {
            piece_id: PieceId(piece),
            point_id: PointId(point),
            to,
        }
    }

    #[test]
    fn move_changes_one_point_and_connected_segments_follow() {
        let mut doc = Document::sample_skirt();
        let before = doc.clone();
        let target = MmPoint::new(-25.5, 10.25);
        doc.apply(move_to(1, 1, target)).unwrap();
        let piece = doc.piece(PieceId(1)).unwrap();
        assert_eq!(
            piece.point(piece.segments[0].start).unwrap().position,
            target
        );
        assert_eq!(piece.point(piece.segments[3].end).unwrap().position, target);
        assert_eq!(piece.points[1..], before.pieces[0].points[1..]);
        assert_eq!(piece.segments, before.pieces[0].segments);
        assert_eq!(piece.metadata, before.pieces[0].metadata);
        assert_eq!(piece.grainline, before.pieces[0].grainline);
        assert_eq!(Document::from_json(&doc.to_json().unwrap()).unwrap(), doc);
        // Absolute commands are idempotent and independent of provider/viewport.
        let after = doc.clone();
        doc.apply(move_to(1, 1, target)).unwrap();
        assert_eq!(doc, after);
    }

    #[test]
    fn rejected_commands_leave_document_unchanged() {
        let mut doc = Document::sample_skirt();
        let before = doc.clone();
        for command in [
            move_to(99, 1, MmPoint::new(0.0, 0.0)),
            move_to(1, 99, MmPoint::new(0.0, 0.0)),
            move_to(1, 1, MmPoint::new(f64::NAN, 0.0)),
            move_to(1, 1, MmPoint::new(0.0, f64::INFINITY)),
        ] {
            assert!(doc.apply(command).is_err());
            assert_eq!(doc, before);
        }
        doc.schema_version = 99;
        let before = doc.clone();
        assert!(doc.apply(move_to(1, 1, MmPoint::new(42.0, 42.0))).is_err());
        assert_eq!(doc, before);
    }

    #[test]
    fn provider_command_json_uses_same_edit_boundary() {
        let json = r#"{"type":"move_point","piece_id":1,"point_id":2,"to":{"x":425.0,"y":-10.0}}"#;
        let command: PatternCommand = serde_json::from_str(json).unwrap();
        let mut doc = Document::sample_skirt();
        doc.apply(command).unwrap();
        assert_eq!(doc.pieces[0].points[1].position, MmPoint::new(425.0, -10.0));
        assert!(
            serde_json::from_str::<PatternCommand>(&json.replace("move_point", "delete_piece"))
                .is_err()
        );
    }

    #[test]
    fn relative_move_accumulates_and_preserves_connections_and_other_geometry() {
        let mut doc = Document::sample_skirt();
        doc.apply(move_to(1, 1, MmPoint::new(-12.5, 3.25))).unwrap();
        let before = doc.clone();
        let command = PatternCommand::TranslatePoint {
            piece_id: PieceId(1),
            point_id: PointId(1),
            delta: MmPoint::new(10.25, -5.5),
        };
        doc.apply(command.clone()).unwrap();
        assert_eq!(doc.pieces[0].points[0].position, MmPoint::new(-2.25, -2.25));
        doc.apply(command).unwrap();
        let mut expected = before.clone();
        expected.pieces[0].points[0].position = MmPoint::new(8.0, -7.75);
        assert_eq!(doc, expected);
        let piece = &doc.pieces[0];
        for id in [piece.segments[0].start, piece.segments[3].end] {
            assert_eq!(piece.point(id).unwrap().position, MmPoint::new(8.0, -7.75));
        }
        doc.apply(PatternCommand::TranslatePoint {
            piece_id: PieceId(1),
            point_id: PointId(1),
            delta: MmPoint::new(0.0, 0.0),
        })
        .unwrap();
        assert_eq!(doc, expected);
        assert_eq!(Document::from_json(&doc.to_json().unwrap()).unwrap(), doc);
    }

    #[test]
    fn relative_move_rejects_bad_ids_offsets_overflow_and_invalid_documents_atomically() {
        let mut doc = Document::sample_skirt();
        doc.apply(move_to(1, 1, MmPoint::new(f64::MAX, -f64::MAX)))
            .unwrap();
        let before = doc.clone();
        for (piece, point, delta) in [
            (99, 1, MmPoint::new(10.0, 0.0)),
            (1, 99, MmPoint::new(10.0, 0.0)),
            (1, 1, MmPoint::new(f64::NAN, 0.0)),
            (1, 1, MmPoint::new(0.0, f64::NAN)),
            (1, 1, MmPoint::new(f64::INFINITY, 0.0)),
            (1, 1, MmPoint::new(0.0, f64::NEG_INFINITY)),
            (1, 1, MmPoint::new(f64::MAX, 0.0)),
            (1, 1, MmPoint::new(0.0, -f64::MAX)),
        ] {
            assert!(
                doc.apply(PatternCommand::TranslatePoint {
                    piece_id: PieceId(piece),
                    point_id: PointId(point),
                    delta,
                })
                .is_err()
            );
            assert_eq!(doc, before);
        }
        doc.pieces[0].segments[0].end = PointId(999);
        let before = doc.clone();
        assert!(
            doc.apply(PatternCommand::TranslatePoint {
                piece_id: PieceId(1),
                point_id: PointId(2),
                delta: MmPoint::new(1.0, 1.0),
            })
            .is_err()
        );
        assert_eq!(doc, before);
    }

    #[test]
    fn relative_command_json_round_trips_and_rejects_unknown_fields() {
        let json =
            r#"{"type":"translate_point","piece_id":1,"point_id":2,"delta":{"x":10.0,"y":-5.0}}"#;
        let command: PatternCommand = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&command).unwrap(), json);
        let mut doc = Document::sample_skirt();
        doc.apply(command).unwrap();
        assert_eq!(doc.pieces[0].points[1].position, MmPoint::new(410.0, -5.0));
        assert!(
            serde_json::from_str::<PatternCommand>(
                &json.replace("\"piece_id\":1", "\"units\":\"pixels\",\"piece_id\":1")
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<PatternCommand>(&json.replace("\"delta\"", "\"to\"")).is_err()
        );
    }

    #[test]
    fn moving_a_curve_handle_updates_only_its_point_and_keeps_connectivity() {
        let mut document = Document::sample_curved_skirt();
        let original = document.clone();
        let target = MmPoint::new(300.0, 440.0);
        document.apply(move_to(1, 5, target)).unwrap();
        let piece = &document.pieces[0];
        assert_eq!(piece.beziers, original.pieces[0].beziers);
        let controls = piece.bezier_controls(&piece.beziers[0]).unwrap();
        assert_eq!(controls[1], target);
        assert_eq!(
            crate::cubic_bezier_point(controls, 0.5).unwrap(),
            MmPoint::new(200.0, 510.0)
        );
        let mut expected = original;
        expected.pieces[0].points[4].position = target;
        assert_eq!(document, expected);
        assert_eq!(
            Document::from_json(&document.to_json().unwrap()).unwrap(),
            document
        );

        let before = document.clone();
        assert!(
            document
                .apply(move_to(1, 5, MmPoint::new(f64::NAN, 440.0)))
                .is_err()
        );
        assert_eq!(document, before);
        document.pieces[0].beziers[0].end = PointId(999);
        let before = document.clone();
        assert!(
            document
                .apply(move_to(1, 5, MmPoint::new(1.0, 2.0)))
                .is_err()
        );
        assert_eq!(document, before);
    }
}
