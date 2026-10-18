use eframe::egui::{self, Color32, RichText};
use pattern_core::{
    CanvasPoint, Document, MmPoint, PatternCommand, PieceId, PointId, ViewTransform,
};
use std::path::Path;

use crate::{canvas::Drag, storage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    pub piece: PieceId,
    pub point: PointId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PointEdit {
    selection: Selection,
    from: MmPoint,
    to: MmPoint,
}

pub(crate) struct PatternApp {
    pub document: Document,
    pub view: ViewTransform,
    pub selected: Option<Selection>,
    pub edit_position: MmPoint,
    pub edit_delta: MmPoint,
    pub drag: Option<Drag>,
    pub fit_pending: bool,
    pub dirty: bool,
    pub(crate) path: String,
    pub(crate) status: String,
    pub(crate) error: bool,
    pub(crate) undo_edits: Vec<PointEdit>,
    pub(crate) redo_edits: Vec<PointEdit>,
    pub(crate) saved_document: Document,
}

impl Default for PatternApp {
    fn default() -> Self {
        let document = Document::sample_skirt();
        Self {
            saved_document: document.clone(),
            document,
            view: ViewTransform::new(CanvasPoint::new(60.0, 680.0), 1.0)
                .expect("Valid initial view"),
            selected: None,
            edit_position: MmPoint::new(0.0, 0.0),
            edit_delta: MmPoint::new(0.0, 0.0),
            drag: None,
            fit_pending: true,
            dirty: false,
            path: "pattern.json".into(),
            status: "Sample skirt panel · 400 × 600 mm".into(),
            error: false,
            undo_edits: Vec::new(),
            redo_edits: Vec::new(),
        }
    }
}

impl PatternApp {
    pub(crate) fn select(&mut self, selection: Option<Selection>) {
        if self.selected != selection {
            self.edit_delta = MmPoint::new(0.0, 0.0);
        }
        self.selected = selection;
        if let Some(point) = self.selected_point() {
            self.edit_position = point;
        }
    }

    fn selected_point(&self) -> Option<MmPoint> {
        let selection = self.selected?;
        self.point_position(selection)
    }

    fn point_position(&self, selection: Selection) -> Option<MmPoint> {
        Some(
            self.document
                .piece(selection.piece)?
                .point(selection.point)?
                .position,
        )
    }

    pub(crate) fn move_point(&mut self, selection: Selection, to: MmPoint) {
        self.apply_move(selection, to, true);
    }

    pub(crate) fn drag_point(&mut self, selection: Selection, to: MmPoint) {
        self.apply_move(selection, to, false);
    }

    fn apply_move(&mut self, selection: Selection, to: MmPoint, record: bool) {
        let from = self.point_position(selection);
        if from == Some(to) {
            return;
        }
        match self.document.apply(PatternCommand::MovePoint {
            piece_id: selection.piece,
            point_id: selection.point,
            to,
        }) {
            Ok(()) => {
                if record && let Some(from) = from {
                    self.record_edit(selection, from, to);
                }
                self.dirty = self.document != self.saved_document;
                self.edit_position = to;
                self.status = format!(
                    "Moved point {} to ({:.2}, {:.2}) mm",
                    selection.point.0, to.x, to.y
                );
                self.error = false;
            }
            Err(error) => self.report_error(error.to_string()),
        }
    }

    fn record_edit(&mut self, selection: Selection, from: MmPoint, to: MmPoint) {
        if from == to {
            return;
        }
        const MAX_EDITS: usize = 100;
        if self.undo_edits.len() == MAX_EDITS {
            self.undo_edits.remove(0);
        }
        self.undo_edits.push(PointEdit {
            selection,
            from,
            to,
        });
        self.redo_edits.clear();
    }

    pub(crate) fn finish_point_drag(&mut self, selection: Selection, from: MmPoint) {
        if let Some(to) = self.point_position(selection) {
            self.record_edit(selection, from, to);
        }
    }

    pub(crate) fn can_undo(&self) -> bool {
        self.drag.is_none() && !self.undo_edits.is_empty()
    }

    fn can_redo(&self) -> bool {
        self.drag.is_none() && !self.redo_edits.is_empty()
    }

    pub(crate) fn undo(&mut self) {
        self.restore_edit(false);
    }

    fn redo(&mut self) {
        self.restore_edit(true);
    }

    fn restore_edit(&mut self, redo: bool) {
        if self.drag.is_some() {
            return;
        }
        let edits = if redo {
            &self.redo_edits
        } else {
            &self.undo_edits
        };
        let Some(edit) = edits.last().copied() else {
            return;
        };
        let to = if redo { edit.to } else { edit.from };
        match self.document.apply(PatternCommand::MovePoint {
            piece_id: edit.selection.piece,
            point_id: edit.selection.point,
            to,
        }) {
            Ok(()) => {
                if redo {
                    self.redo_edits.pop();
                    self.undo_edits.push(edit);
                } else {
                    self.undo_edits.pop();
                    self.redo_edits.push(edit);
                }
                if let Some(point) = self.selected_point() {
                    self.edit_position = point;
                }
                self.dirty = self.document != self.saved_document;
                self.status = format!(
                    "{} move of point {}",
                    if redo { "Redid" } else { "Undid" },
                    edit.selection.point.0
                );
                self.error = false;
            }
            Err(error) => self.report_error(error.to_string()),
        }
    }

    fn history_shortcuts(&mut self, ctx: &egui::Context) {
        if self.drag.is_some() || ctx.text_edit_focused() {
            return;
        }
        let redo = egui::KeyboardShortcut::new(
            egui::Modifiers {
                command: true,
                shift: true,
                ..Default::default()
            },
            egui::Key::Z,
        );
        let redo_y = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
        let undo = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
        if ctx.input_mut(|input| input.consume_shortcut(&redo) || input.consume_shortcut(&redo_y)) {
            self.redo();
        } else if ctx.input_mut(|input| input.consume_shortcut(&undo)) {
            self.undo();
        }
    }

    pub(crate) fn report_error(&mut self, error: String) {
        self.status = error;
        self.error = true;
    }

    fn offset_selected_point(&mut self) {
        let Some(selection) = self.selected else {
            return;
        };
        if self.edit_delta == MmPoint::new(0.0, 0.0) {
            return;
        }
        let from = self.point_position(selection);
        match self.document.apply(PatternCommand::TranslatePoint {
            piece_id: selection.piece,
            point_id: selection.point,
            delta: self.edit_delta,
        }) {
            Ok(()) => {
                if let Some(point) = self.selected_point() {
                    if let Some(from) = from {
                        self.record_edit(selection, from, point);
                    }
                    self.edit_position = point;
                }
                self.dirty = self.document != self.saved_document;
                self.status = format!(
                    "Offset point {} by ({:.2}, {:.2}) mm",
                    selection.point.0, self.edit_delta.x, self.edit_delta.y
                );
                self.error = false;
            }
            Err(error) => self.report_error(error.to_string()),
        }
    }

    fn save(&mut self) {
        match storage::save(&self.document, Path::new(&self.path)) {
            Ok(()) => {
                self.saved_document = self.document.clone();
                self.dirty = false;
                self.status = format!("Saved {}", self.path);
                self.error = false;
            }
            Err(error) => self.report_error(error),
        }
    }

    fn load(&mut self) {
        match storage::load(Path::new(&self.path)) {
            Ok(document) => {
                self.saved_document = document.clone();
                self.document = document;
                self.undo_edits.clear();
                self.redo_edits.clear();
                self.select(None);
                self.drag = None;
                self.fit_pending = true;
                self.dirty = false;
                self.status = format!("Loaded {}", self.path);
                self.error = false;
            }
            Err(error) => self.report_error(error),
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("SEAMETRIC")
                        .strong()
                        .size(22.0)
                        .color(Color32::from_rgb(106, 218, 201)),
                );
                ui.label("Pattern CAD / 0.1");
                ui.separator();
                if ui
                    .add_enabled(self.can_undo(), egui::Button::new("Undo"))
                    .on_hover_text("Undo the last point edit (Cmd/Ctrl+Z)")
                    .clicked()
                {
                    self.undo();
                }
                if ui
                    .add_enabled(self.can_redo(), egui::Button::new("Redo"))
                    .on_hover_text("Redo a point edit (Cmd/Ctrl+Shift+Z or Cmd/Ctrl+Y)")
                    .clicked()
                {
                    self.redo();
                }
                if ui.button("Fit piece").clicked() {
                    self.fit_pending = true;
                }
                ui.label(format!("{:.2} display units/mm", self.view.scale()));
                if self.dirty {
                    ui.label(RichText::new("● Unsaved").color(Color32::from_rgb(243, 188, 105)));
                }
            });
            ui.horizontal(|ui| {
                ui.label("JSON path");
                ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(360.0));
                if ui.button("Save").clicked() {
                    self.save();
                }
                if ui
                    .button("Load")
                    .on_hover_text("Replace current document; save your edits first")
                    .clicked()
                {
                    self.load();
                }
            });
            ui.add_space(6.0);
        });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        egui::Panel::right("inspector")
            .exact_size(240.0)
            .show(ui, |ui| {
                ui.add_space(12.0);
                ui.heading("Pattern document");
                ui.label("All geometry in millimeters");
                ui.separator();
                for piece in &self.document.pieces {
                    ui.label(RichText::new(&piece.metadata.name).strong());
                    ui.label(format!(
                        "Piece {} · Cut {} · {}",
                        piece.id.0, piece.metadata.cut_quantity, piece.metadata.fabric
                    ));
                    ui.label(format!(
                        "{} points · {} segments",
                        piece.points.len(),
                        piece.segments.len()
                    ));
                    ui.label(&piece.metadata.notes);
                    ui.add_space(8.0);
                }
                ui.separator();
                ui.heading("Selected point");
                if let Some(selection) = self.selected {
                    ui.label(format!(
                        "Piece {} / Point {}",
                        selection.piece.0, selection.point.0
                    ));
                    ui.horizontal(|ui| {
                        ui.label("X (mm)");
                        ui.add(
                            egui::DragValue::new(&mut self.edit_position.x)
                                .speed(0.5)
                                .max_decimals(3),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Y (mm)");
                        ui.add(
                            egui::DragValue::new(&mut self.edit_position.y)
                                .speed(0.5)
                                .max_decimals(3),
                        );
                    });
                    if ui.button("Apply coordinates").clicked() {
                        self.move_point(selection, self.edit_position);
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("Relative move").strong());
                    ui.horizontal(|ui| {
                        ui.label("dX (mm)");
                        ui.add(
                            egui::DragValue::new(&mut self.edit_delta.x)
                                .speed(0.5)
                                .max_decimals(3),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("dY (mm)");
                        ui.add(
                            egui::DragValue::new(&mut self.edit_delta.y)
                                .speed(0.5)
                                .max_decimals(3),
                        );
                    });
                    if ui
                        .button("Offset point")
                        .on_hover_text(
                            "Add dX/dY to the current point position; positive Y moves up",
                        )
                        .clicked()
                    {
                        self.offset_selected_point();
                    }
                    if ui.button("Deselect").clicked() {
                        self.select(None);
                    }
                } else {
                    ui.label("Click a corner to select it.");
                }
                ui.add_space(16.0);
                ui.separator();
                ui.label(RichText::new("Canvas controls").strong());
                ui.label("Drag a point to move");
                ui.label("Drag empty space to pan");
                ui.label("Right / middle drag to pan");
                ui.label("Scroll / pinch to zoom");
                ui.label("X right · Y up");
                ui.add_space(12.0);
                ui.label("Grainline shown in amber");
            });
    }
}

impl eframe::App for PatternApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.history_shortcuts(ui.ctx());
        self.toolbar(ui);
        egui::Panel::bottom("status").show(ui, |ui| {
            let color = if self.error {
                Color32::from_rgb(255, 140, 140)
            } else {
                Color32::from_gray(195)
            };
            ui.label(RichText::new(&self.status).color(color));
        });
        self.inspector(ui);
        egui::CentralPanel::default().show(ui, |ui| {
            self.canvas(ui);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_point() -> Selection {
        Selection {
            piece: PieceId(1),
            point: PointId(1),
        }
    }

    #[test]
    fn absolute_and_relative_edits_undo_and_redo_in_order_without_changing_view() {
        let mut app = PatternApp::default();
        app.select(Some(first_point()));
        let original = app.document.clone();
        let view = app.view;
        app.move_point(first_point(), MmPoint::new(-7.25, 3.5));
        let absolute = app.document.clone();
        app.edit_delta = MmPoint::new(10.0, -5.0);
        app.offset_selected_point();
        let relative = app.document.clone();
        app.undo();
        assert_eq!(app.document, absolute);
        assert_eq!(app.edit_position, MmPoint::new(-7.25, 3.5));
        assert!(app.dirty);
        app.undo();
        assert_eq!(app.document, original);
        assert!(!app.dirty);
        assert!(!app.can_undo());
        app.redo();
        assert_eq!(app.document, absolute);
        app.redo();
        assert_eq!(app.document, relative);
        assert_eq!(app.edit_position, MmPoint::new(2.75, -1.5));
        assert!(!app.can_redo());
        assert!(app.dirty);
        assert_eq!(app.view, view);
        assert_eq!(app.selected, Some(first_point()));
    }

    #[test]
    fn only_a_successful_new_edit_discards_redo() {
        let mut app = PatternApp::default();
        app.select(Some(first_point()));
        app.move_point(first_point(), MmPoint::new(10.0, 20.0));
        app.undo();
        app.move_point(first_point(), MmPoint::new(0.0, 0.0));
        app.edit_delta = MmPoint::new(0.0, 0.0);
        app.offset_selected_point();
        app.edit_delta = MmPoint::new(f64::INFINITY, 0.0);
        app.offset_selected_point();
        assert!(!app.can_undo());
        assert!(app.can_redo());
        app.redo();
        assert_eq!(app.edit_position, MmPoint::new(10.0, 20.0));
        app.undo();
        app.move_point(first_point(), MmPoint::new(30.0, 40.0));
        assert!(!app.can_redo());
        app.undo();
        assert_eq!(app.edit_position, MmPoint::new(0.0, 0.0));
    }

    #[test]
    fn save_tracks_clean_position_and_successful_load_clears_history() {
        let path =
            std::env::temp_dir().join(format!("seametric-history-{}.json", std::process::id()));
        let mut app = PatternApp {
            path: path.to_string_lossy().into_owned(),
            ..Default::default()
        };
        app.select(Some(first_point()));
        app.move_point(first_point(), MmPoint::new(10.0, 20.0));
        app.save();
        assert!(!app.error);
        assert!(!app.dirty);
        app.undo();
        assert!(app.dirty);
        app.redo();
        assert!(!app.dirty);
        app.move_point(first_point(), MmPoint::new(30.0, 40.0));
        app.undo();
        assert!(!app.dirty);
        assert!(app.can_undo());
        assert!(app.can_redo());
        app.load();
        assert!(!app.error);
        assert!(!app.dirty);
        assert!(app.selected.is_none());
        assert!(!app.can_undo());
        assert!(!app.can_redo());
        assert_eq!(
            app.document.pieces[0].points[0].position,
            MmPoint::new(10.0, 20.0)
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_history_command_does_not_pop_the_edit() {
        let mut app = PatternApp::default();
        app.move_point(first_point(), MmPoint::new(10.0, 20.0));
        app.document.schema_version = 99;
        let document = app.document.clone();
        let edits = app.undo_edits.clone();
        app.undo();
        assert_eq!(app.document, document);
        assert_eq!(app.undo_edits, edits);
        assert!(app.redo_edits.is_empty());
        assert!(app.error);
        app.document.schema_version = Document::SCHEMA_VERSION;
        app.undo();
        assert_eq!(
            app.document.pieces[0].points[0].position,
            MmPoint::new(0.0, 0.0)
        );
    }

    #[test]
    fn history_is_bounded_to_last_one_hundred_edits() {
        let mut app = PatternApp::default();
        for x in 1..=105 {
            app.move_point(first_point(), MmPoint::new(f64::from(x), 0.0));
        }
        assert_eq!(app.undo_edits.len(), 100);
        while app.can_undo() {
            app.undo();
        }
        assert_eq!(
            app.document.pieces[0].points[0].position,
            MmPoint::new(5.0, 0.0)
        );
        while app.can_redo() {
            app.redo();
        }
        assert_eq!(
            app.document.pieces[0].points[0].position,
            MmPoint::new(105.0, 0.0)
        );
    }

    #[test]
    fn keyboard_shortcuts_respect_text_edit_focus() {
        let ctx = egui::Context::default();
        let mut app = PatternApp::default();
        app.move_point(first_point(), MmPoint::new(10.0, 20.0));
        let mut text = String::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.add(egui::TextEdit::singleline(&mut text).id(egui::Id::new("focus-test")))
                .request_focus();
        })
        .drop_without_applying_deltas();
        assert!(ctx.text_edit_focused());
        let modifiers = egui::Modifiers::COMMAND;
        let event = egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        ctx.run_ui(
            egui::RawInput {
                events: vec![event.clone()],
                ..Default::default()
            },
            |ui| {
                app.history_shortcuts(ui.ctx());
                ui.memory_mut(|memory| memory.surrender_focus(egui::Id::new("focus-test")));
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(
            app.document.pieces[0].points[0].position,
            MmPoint::new(10.0, 20.0)
        );
        ctx.run_ui(
            egui::RawInput {
                events: vec![event],
                ..Default::default()
            },
            |ui| {
                app.history_shortcuts(ui.ctx());
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(
            app.document.pieces[0].points[0].position,
            MmPoint::new(0.0, 0.0)
        );
        let modifiers = egui::Modifiers {
            command: true,
            shift: true,
            ..Default::default()
        };
        ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ui| {
                app.history_shortcuts(ui.ctx());
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(
            app.document.pieces[0].points[0].position,
            MmPoint::new(10.0, 20.0)
        );
    }

    #[test]
    fn relative_move_uses_current_geometry_keeps_view_and_refreshes_inspector() {
        let mut app = PatternApp::default();
        let selection = Selection {
            piece: PieceId(1),
            point: PointId(2),
        };
        app.select(Some(selection));
        let view = app.view;
        // An uncommitted absolute-coordinate draft is not the translation origin.
        app.edit_position = MmPoint::new(999.0, 999.0);
        app.edit_delta = MmPoint::new(10.0, -5.0);
        app.offset_selected_point();
        assert_eq!(app.selected_point(), Some(MmPoint::new(410.0, -5.0)));
        assert_eq!(app.edit_position, MmPoint::new(410.0, -5.0));
        app.offset_selected_point();
        assert_eq!(app.selected_point(), Some(MmPoint::new(420.0, -10.0)));
        assert_eq!(app.view, view);
        assert_eq!(app.selected, Some(selection));
        assert!(app.dirty);
        assert!(!app.error);
        app.select(Some(Selection {
            piece: PieceId(1),
            point: PointId(1),
        }));
        assert_eq!(app.edit_delta, MmPoint::new(0.0, 0.0));
    }

    #[test]
    fn rejected_relative_move_preserves_document_and_ui_edit_state() {
        let mut app = PatternApp::default();
        let selection = Selection {
            piece: PieceId(1),
            point: PointId(2),
        };
        app.select(Some(selection));
        let before = app.document.clone();
        app.edit_delta = MmPoint::new(f64::INFINITY, 0.0);
        app.offset_selected_point();
        assert_eq!(app.document, before);
        assert_eq!(app.edit_position, MmPoint::new(400.0, 0.0));
        assert_eq!(app.selected, Some(selection));
        assert!(!app.dirty);
        assert!(app.error);
    }

    #[test]
    fn failed_load_preserves_active_document_selection_and_unsaved_edits() {
        let path = std::env::temp_dir().join(format!(
            "seametric-invalid-load-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, "{invalid").unwrap();
        let mut app = PatternApp {
            path: path.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let selection = Selection {
            piece: PieceId(1),
            point: PointId(1),
        };
        app.select(Some(selection));
        app.move_point(selection, MmPoint::new(42.0, -3.0));
        let before = app.document.clone();
        let history = app.undo_edits.clone();
        app.load();
        assert_eq!(app.document, before);
        assert_eq!(app.selected, Some(selection));
        assert!(app.dirty);
        assert!(app.error);
        assert_eq!(app.undo_edits, history);
        std::fs::remove_file(path).unwrap();
    }
}
