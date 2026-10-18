use crate::app::{PatternApp, Selection};
use eframe::egui::{self, Color32, PointerButton, Pos2, Rect, Sense, Stroke, Vec2};
use pattern_core::{CanvasPoint, MmPoint, ViewTransform, cubic_bezier_point};

#[derive(Clone, Copy)]
pub(crate) struct Drag {
    button: PointerButton,
    target: DragTarget,
    last_pos: Pos2,
}

#[derive(Clone, Copy)]
enum DragTarget {
    Pan,
    Point {
        selection: Selection,
        offset: MmPoint,
        from: MmPoint,
    },
}

fn local(rect: Rect, pos: Pos2) -> CanvasPoint {
    CanvasPoint::new(
        f64::from(pos.x - rect.left()),
        f64::from(pos.y - rect.top()),
    )
}

/// Bound drawing work while increasing sampling density with the projected control polygon.
fn bezier_polyline(controls: [MmPoint; 4], scale: f64) -> Option<Vec<MmPoint>> {
    let polygon_length: f64 = controls
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y))
        .sum();
    let steps = (polygon_length * scale / 6.0).ceil().clamp(16.0, 1024.0) as usize;
    (0..=steps)
        .map(|step| cubic_bezier_point(controls, step as f64 / steps as f64))
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

impl PatternApp {
    fn screen(&self, rect: Rect, point: MmPoint) -> Pos2 {
        let canvas = self.view.model_to_canvas(point);
        rect.min + Vec2::new(canvas.x as f32, canvas.y as f32)
    }

    fn fit(&mut self, rect: Rect) {
        let mut min = MmPoint::new(f64::INFINITY, f64::INFINITY);
        let mut max = MmPoint::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for piece in &self.document.pieces {
            for point in piece
                .points
                .iter()
                .map(|p| p.position)
                .chain([piece.grainline.start, piece.grainline.end])
            {
                min.x = min.x.min(point.x);
                min.y = min.y.min(point.y);
                max.x = max.x.max(point.x);
                max.y = max.y.max(point.y);
            }
        }
        if !min.is_finite() {
            min = MmPoint::new(0.0, 0.0);
            max = MmPoint::new(400.0, 600.0);
        }
        let width = f64::from(rect.width());
        let height = f64::from(rect.height());
        let scale = ((width - 100.0).max(1.0) / (max.x - min.x).max(1.0))
            .min((height - 100.0).max(1.0) / (max.y - min.y).max(1.0))
            .clamp(ViewTransform::MIN_SCALE, ViewTransform::MAX_SCALE);
        let origin = CanvasPoint::new(
            width / 2.0 - (min.x / 2.0 + max.x / 2.0) * scale,
            height / 2.0 + (min.y / 2.0 + max.y / 2.0) * scale,
        );
        match ViewTransform::new(origin, scale) {
            Ok(view) => self.view = view,
            Err(e) => self.report_error(e.to_string()),
        }
        self.fit_pending = false;
    }

    fn hit_point(&self, rect: Rect, pos: Pos2) -> Option<Selection> {
        let mut nearest = None;
        let mut distance = 12.0_f32.powi(2);
        for piece in &self.document.pieces {
            for point in &piece.points {
                let candidate = self.screen(rect, point.position).distance_sq(pos);
                if candidate < distance {
                    distance = candidate;
                    nearest = Some(Selection {
                        piece: piece.id,
                        point: point.id,
                    });
                }
            }
        }
        nearest
    }

    pub(crate) fn canvas(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        if self.fit_pending {
            self.fit(rect);
        }
        let (pos, pressed, scroll, zoom) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.events.iter().find_map(|event| match event {
                    egui::Event::PointerButton {
                        pos,
                        button,
                        pressed: true,
                        ..
                    } if matches!(
                        button,
                        PointerButton::Primary | PointerButton::Secondary | PointerButton::Middle
                    ) =>
                    {
                        Some((*button, *pos))
                    }
                    _ => None,
                }),
                i.smooth_scroll_delta.y,
                i.zoom_delta(),
            )
        });

        if response.hovered()
            && self.drag.is_none()
            && let Some(pos) = pos
        {
            // Egui consumes modifier-scroll into zoom_delta; plain wheel uses scroll.
            let factor = f64::from(zoom) * (f64::from(scroll) * 0.002).exp();
            if factor != 1.0
                && let Err(e) = self.view.zoom_at(local(rect, pos), factor)
            {
                self.report_error(e.to_string());
            }
            if let Some((button, press_pos)) = pressed {
                let target = if button == PointerButton::Primary {
                    let selection = self.hit_point(rect, press_pos);
                    self.select(selection);
                    if let Some(selection) = selection {
                        let cursor = self.view.canvas_to_model(local(rect, press_pos));
                        DragTarget::Point {
                            selection,
                            from: self.edit_position,
                            offset: MmPoint::new(
                                self.edit_position.x - cursor.x,
                                self.edit_position.y - cursor.y,
                            ),
                        }
                    } else {
                        DragTarget::Pan
                    }
                } else {
                    DragTarget::Pan
                };
                self.drag = Some(Drag {
                    button,
                    target,
                    last_pos: press_pos,
                });
            }
        }

        if let Some(mut drag) = self.drag {
            // Own the gesture from press to release; retain motion in the release frame,
            // even when egui's click/drag threshold has not yet been crossed.
            let (down, released) = ui.input(|i| {
                (
                    i.pointer.button_down(drag.button),
                    i.pointer.button_released(drag.button),
                )
            });
            if (down || released)
                && let Some(pos) = pos
            {
                match drag.target {
                    DragTarget::Pan => {
                        let delta = pos - drag.last_pos;
                        if let Err(e) = self
                            .view
                            .pan(CanvasPoint::new(f64::from(delta.x), f64::from(delta.y)))
                        {
                            self.report_error(e.to_string());
                        }
                    }
                    DragTarget::Point {
                        selection, offset, ..
                    } => {
                        let cursor = self.view.canvas_to_model(local(rect, pos));
                        self.drag_point(
                            selection,
                            MmPoint::new(cursor.x + offset.x, cursor.y + offset.y),
                        );
                    }
                }
                drag.last_pos = pos;
            }
            if !down
                && let DragTarget::Point {
                    selection, from, ..
                } = drag.target
            {
                self.finish_point_drag(selection, from);
            }
            self.drag = down.then_some(drag);
        }

        if response.hovered() {
            let cursor = if self.drag.is_some() {
                egui::CursorIcon::Grabbing
            } else if pos.is_some_and(|p| self.hit_point(rect, p).is_some()) {
                egui::CursorIcon::PointingHand
            } else {
                egui::CursorIcon::Grab
            };
            ui.ctx().set_cursor_icon(cursor);
        }
        self.paint(ui.painter().with_clip_rect(rect), rect);
    }

    fn paint(&self, painter: egui::Painter, rect: Rect) {
        let ink = Color32::from_rgb(34, 79, 86);
        let accent = Color32::from_rgb(10, 139, 129);
        let grain = Color32::from_rgb(187, 122, 40);
        let handle_color = Color32::from_rgb(131, 102, 180);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(244, 243, 235));

        // Adaptive grid, bounded by the visible canvas (never the document's full extents).
        let step = 10_f64.powf((55.0 / self.view.scale()).log10().ceil());
        let top_left = self.view.canvas_to_model(CanvasPoint::new(0.0, 0.0));
        let bottom_right = self.view.canvas_to_model(CanvasPoint::new(
            f64::from(rect.width()),
            f64::from(rect.height()),
        ));
        let grid = Stroke::new(1.0, Color32::from_rgb(223, 226, 219));
        let start_x = (top_left.x / step).ceil() * step;
        let start_y = (bottom_right.y / step).ceil() * step;
        for i in 0..((bottom_right.x - start_x) / step).ceil().max(0.0) as usize {
            let x = start_x + i as f64 * step;
            painter.line_segment(
                [
                    self.screen(rect, MmPoint::new(x, top_left.y)),
                    self.screen(rect, MmPoint::new(x, bottom_right.y)),
                ],
                grid,
            );
        }
        for i in 0..((top_left.y - start_y) / step).ceil().max(0.0) as usize {
            let y = start_y + i as f64 * step;
            painter.line_segment(
                [
                    self.screen(rect, MmPoint::new(top_left.x, y)),
                    self.screen(rect, MmPoint::new(bottom_right.x, y)),
                ],
                grid,
            );
        }

        for piece in &self.document.pieces {
            for curve in &piece.beziers {
                if let Some(controls) = piece.bezier_controls(curve)
                    && let Some(points) = bezier_polyline(controls, self.view.scale())
                {
                    for [anchor, handle] in [[controls[0], controls[1]], [controls[3], controls[2]]]
                    {
                        painter.line_segment(
                            [self.screen(rect, anchor), self.screen(rect, handle)],
                            Stroke::new(1.0, handle_color),
                        );
                    }
                    painter.add(egui::Shape::line(
                        points
                            .into_iter()
                            .map(|point| self.screen(rect, point))
                            .collect(),
                        Stroke::new(2.5, ink),
                    ));
                }
            }
            for segment in &piece.segments {
                if let (Some(start), Some(end)) =
                    (piece.point(segment.start), piece.point(segment.end))
                {
                    let a = self.screen(rect, start.position);
                    let b = self.screen(rect, end.position);
                    painter.line_segment([a, b], Stroke::new(2.5, ink));
                    if a.distance(b) > 75.0 {
                        let length = (end.position.x - start.position.x)
                            .hypot(end.position.y - start.position.y);
                        painter.text(
                            a.lerp(b, 0.5) + Vec2::new(12.0, -12.0),
                            egui::Align2::CENTER_BOTTOM,
                            format!("{length:.1} mm"),
                            egui::FontId::proportional(12.0),
                            ink,
                        );
                    }
                }
            }
            let start = self.screen(rect, piece.grainline.start);
            let end = self.screen(rect, piece.grainline.end);
            painter.arrow(start, end - start, Stroke::new(2.0, grain));
            painter.text(
                start.lerp(end, 0.5) + Vec2::new(12.0, 0.0),
                egui::Align2::LEFT_CENTER,
                "GRAIN",
                egui::FontId::proportional(12.0),
                grain,
            );
            for point in &piece.points {
                let selected = self.selected
                    == Some(Selection {
                        piece: piece.id,
                        point: point.id,
                    });
                let pos = self.screen(rect, point.position);
                let handle = piece.is_bezier_handle(point.id);
                if selected {
                    painter.circle_stroke(pos, 10.0, Stroke::new(2.0, accent));
                }
                if handle {
                    painter.rect_filled(
                        Rect::from_center_size(pos, Vec2::splat(10.0)),
                        1.0,
                        if selected { accent } else { handle_color },
                    );
                } else {
                    painter.circle_filled(pos, 5.0, if selected { accent } else { ink });
                }
                painter.text(
                    pos + Vec2::new(12.0, 12.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}{}", if handle { "H" } else { "P" }, point.id.0),
                    egui::FontId::proportional(12.0),
                    ink,
                );
            }
        }
        painter.text(
            rect.left_top() + Vec2::splat(14.0),
            egui::Align2::LEFT_TOP,
            format!("MILLIMETERS  /  Grid {step:.0} mm"),
            egui::FontId::proportional(12.0),
            ink,
        );
        let bar = rect.left_bottom() + Vec2::new(24.0, -24.0);
        let length = (step * self.view.scale()) as f32;
        painter.line_segment([bar, bar + Vec2::new(length, 0.0)], Stroke::new(2.0, ink));
        painter.text(
            bar + Vec2::new(0.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{step:.0} mm"),
            egui::FontId::proportional(12.0),
            ink,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(app: &mut PatternApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 800.0))),
                events,
                ..Default::default()
            },
            |ui| {
                app.canvas(ui);
            },
        )
        .drop_without_applying_deltas();
    }

    fn button(pos: Pos2, button: PointerButton, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn curve_sampling_preserves_endpoints_and_scales_with_a_bounded_workload() {
        let controls = [
            MmPoint::new(0.0, 0.0),
            MmPoint::new(0.0, 100.0),
            MmPoint::new(100.0, 100.0),
            MmPoint::new(100.0, 0.0),
        ];
        let low_zoom = bezier_polyline(controls, ViewTransform::MIN_SCALE).unwrap();
        let high_zoom = bezier_polyline(controls, ViewTransform::MAX_SCALE).unwrap();
        assert_eq!(low_zoom.first(), Some(&controls[0]));
        assert_eq!(low_zoom.last(), Some(&controls[3]));
        assert_eq!(low_zoom[8], MmPoint::new(50.0, 75.0));
        assert_eq!(low_zoom.len(), 17);
        assert_eq!(high_zoom.len(), 1025);
        assert_eq!(high_zoom.first(), Some(&controls[0]));
        assert_eq!(high_zoom.last(), Some(&controls[3]));
        assert!(high_zoom.iter().all(|point| point.is_finite()));
    }

    #[test]
    fn curve_handles_drag_in_millimeters_at_two_zoom_levels_and_undo_as_one_edit() {
        for scale in [0.5, 2.0] {
            let ctx = egui::Context::default();
            let mut app = PatternApp {
                view: ViewTransform::new(CanvasPoint::new(100.0, 80.0 + 600.0 * scale), scale)
                    .unwrap(),
                fit_pending: false,
                ..Default::default()
            };
            frame(&mut app, &ctx, vec![]);
            let original = app.document.clone();
            let start = Pos2::new((100.0 + 300.0 * scale) as f32, (80.0 + 80.0 * scale) as f32);
            let end = start + Vec2::new(20.0, -30.0);
            frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(start),
                    button(start, PointerButton::Primary, true),
                ],
            );
            assert_eq!(app.selected.unwrap().point, pattern_core::PointId(5));
            frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
            frame(
                &mut app,
                &ctx,
                vec![button(end, PointerButton::Primary, false)],
            );
            let piece = &app.document.pieces[0];
            let controls = piece.bezier_controls(&piece.beziers[0]).unwrap();
            assert_eq!(
                controls[1],
                MmPoint::new(300.0 + 20.0 / scale, 520.0 + 30.0 / scale)
            );
            assert_eq!(piece.beziers, original.pieces[0].beziers);
            assert_eq!(piece.points[..4], original.pieces[0].points[..4]);
            assert_eq!(app.undo_edits.len(), 1);
            app.undo();
            assert_eq!(app.document, original);
            assert!(!app.dirty);
        }
    }

    #[test]
    fn a_drag_spanning_frames_is_one_undo_step_and_a_click_records_nothing() {
        let ctx = egui::Context::default();
        let mut app = PatternApp {
            view: ViewTransform::new(CanvasPoint::new(100.0, 700.0), 1.0).unwrap(),
            fit_pending: false,
            ..Default::default()
        };
        frame(&mut app, &ctx, vec![]);
        let original = app.document.clone();
        let start = Pos2::new(100.0, 700.0);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                button(start, PointerButton::Primary, true),
            ],
        );
        frame(
            &mut app,
            &ctx,
            vec![button(start, PointerButton::Primary, false)],
        );
        assert!(!app.can_undo());
        frame(
            &mut app,
            &ctx,
            vec![button(start, PointerButton::Primary, true)],
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(Pos2::new(120.0, 680.0))],
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(Pos2::new(150.0, 650.0))],
        );
        assert!(!app.can_undo());
        frame(
            &mut app,
            &ctx,
            vec![button(
                Pos2::new(150.0, 650.0),
                PointerButton::Primary,
                false,
            )],
        );
        assert!(app.can_undo());
        assert_eq!(
            app.document.pieces[0].points[0].position,
            MmPoint::new(50.0, 50.0)
        );
        app.undo();
        assert_eq!(app.document, original);
        assert!(!app.can_undo());
        assert!(!app.dirty);
    }

    #[test]
    fn canvas_drag_edits_in_mm_at_two_zoom_levels_without_jump() {
        for scale in [0.5, 2.0] {
            let ctx = egui::Context::default();
            let mut app = PatternApp {
                view: ViewTransform::new(CanvasPoint::new(100.0, 700.0), scale).unwrap(),
                fit_pending: false,
                ..Default::default()
            };
            frame(&mut app, &ctx, vec![]);
            // Start slightly off-center to exercise preservation of the grab offset.
            let start = Pos2::new(103.0, 697.0);
            frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(start),
                    button(start, PointerButton::Primary, true),
                ],
            );
            assert_eq!(
                app.selected,
                Some(Selection {
                    piece: pattern_core::PieceId(1),
                    point: pattern_core::PointId(1)
                })
            );
            assert_eq!(
                app.document.pieces[0].points[0].position,
                MmPoint::new(0.0, 0.0)
            );
            let end = start + Vec2::new(20.0, -30.0);
            frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(end),
                    button(end, PointerButton::Primary, false),
                ],
            );
            assert_eq!(
                app.document.pieces[0].points[0].position,
                MmPoint::new(20.0 / scale, 30.0 / scale)
            );
            assert!(app.dirty);
            assert!(app.drag.is_none());
        }
    }

    #[test]
    fn canvas_handles_a_complete_drag_in_one_frame() {
        let ctx = egui::Context::default();
        let mut app = PatternApp {
            view: ViewTransform::new(CanvasPoint::new(100.0, 700.0), 1.0).unwrap(),
            fit_pending: false,
            ..Default::default()
        };
        frame(&mut app, &ctx, vec![]);
        let start = Pos2::new(100.0, 700.0);
        let end = Pos2::new(140.0, 650.0);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                button(start, PointerButton::Primary, true),
                egui::Event::PointerMoved(end),
                button(end, PointerButton::Primary, false),
            ],
        );
        assert_eq!(
            app.document.pieces[0].points[0].position,
            MmPoint::new(40.0, 50.0)
        );
        assert!(app.drag.is_none());
    }

    #[test]
    fn canvas_pan_and_zoom_leave_document_unchanged() {
        let ctx = egui::Context::default();
        let mut app = PatternApp {
            fit_pending: false,
            ..Default::default()
        };
        let before = app.document.clone();
        frame(&mut app, &ctx, vec![]);
        let start = Pos2::new(750.0, 400.0);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                button(start, PointerButton::Secondary, true),
            ],
        );
        let origin_before = app.view.model_to_canvas(MmPoint::new(0.0, 0.0));
        let end = start + Vec2::new(25.0, 15.0);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
        frame(
            &mut app,
            &ctx,
            vec![button(end, PointerButton::Secondary, false)],
        );
        assert_eq!(
            app.view.model_to_canvas(MmPoint::new(0.0, 0.0)),
            CanvasPoint::new(origin_before.x + 25.0, origin_before.y + 15.0)
        );
        let anchor_before = app
            .view
            .canvas_to_model(CanvasPoint::new(f64::from(end.x), f64::from(end.y)));
        frame(&mut app, &ctx, vec![egui::Event::Zoom(2.0)]);
        assert_eq!(app.view.scale(), 2.0);
        assert_eq!(
            app.view
                .canvas_to_model(CanvasPoint::new(f64::from(end.x), f64::from(end.y))),
            anchor_before
        );
        assert_eq!(app.document, before);
        assert!(!app.dirty);
    }
}
