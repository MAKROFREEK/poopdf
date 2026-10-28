// poopdf - a local-first PDF viewer that reads documents aloud.
// Copyright (C) 2026  poopdf contributors <https://github.com/MAKROFREEK/poopdf>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! The eframe application: keyboard navigation, wheel scrolling, zooming,
//! rendering pages through the bitmap cache, and the page indicator.

use std::path::Path;

use eframe::egui;
use egui::{Align2, ColorImage, Event, Key, TextureHandle, Vec2};

use crate::document::Document;
use crate::navigation::{Event as NavEvent, Geometry, Navigator, StripRect};
use crate::page_cache::{CacheKey, DEFAULT_BUDGET_BYTES, PageCache};
use crate::pdfium_lib;
use crate::view::PageLayout;
use crate::zoom::{STEP, Zoom};

pub struct PdfApp {
    pdfium: &'static pdfium_render::prelude::Pdfium,
    document: Option<Document>,
    error: Option<String>,
    zoom: Zoom,
    /// Paragraph Cursor, Hover Highlight, selection, and the scroll offset.
    navigator: Navigator,
    cache: PageCache<TextureHandle>,
    /// File name most recently applied to the window title.
    last_applied_title: Option<String>,
}

impl PdfApp {
    /// Provision pdfium once, then create the app. A CLI path opens the
    /// Document immediately; on failure we show the error in-app.
    pub fn new(start_path: Option<String>) -> Result<Self, pdfium_lib::BindError> {
        let pdfium = pdfium_lib::bind_pinned_pdfium()?;
        let mut app = Self {
            pdfium,
            document: None,
            error: None,
            zoom: Zoom::default(),
            navigator: Navigator::new(),
            cache: PageCache::new(DEFAULT_BUDGET_BYTES),
            last_applied_title: None,
        };
        if let Some(path) = start_path {
            app.open_document(Path::new(&path));
        }
        Ok(app)
    }

    /// Open (or re-open) the Document, resetting view state.
    fn open_document(&mut self, path: &Path) {
        match Document::open(self.pdfium, path) {
            Ok(document) => {
                // Extract the text layer up front. A failure is surfaced but
                // does not block opening: the Document stays viewable, just
                // with no Paragraphs to navigate.
                let extraction = document.paragraphs();
                self.document = Some(document);
                self.error = None;
                self.zoom = Zoom::default();
                // The cached bitmaps belong to the previous Document; drop
                // all of them, not just other zooms.
                self.cache.clear();
                match extraction {
                    Ok(paragraphs) => self.navigator.set_paragraphs(paragraphs),
                    Err(e) => {
                        self.navigator.set_paragraphs(Vec::new());
                        self.error = Some(format!("Could not read text of {}: {e}", path.display()));
                    }
                }
            }
            Err(e) => self.error = Some(format!("Could not open {}: {e}", path.display())),
        }
    }

    fn open_with_picker(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF document", &["pdf"])
            .pick_file()
        {
            self.open_document(&path);
        }
    }

    /// The current vertical page strip at the current zoom.
    fn layout(&self) -> PageLayout {
        let Some(document) = &self.document else {
            return PageLayout::new(&[], 0.0);
        };
        let zoom = self.zoom.factor();
        let heights: Vec<f32> = document
            .page_sizes_points
            .iter()
            .map(|&(_, h)| h * zoom)
            .collect();
        PageLayout::new(&heights, document.page_spacing_points)
    }

    /// View geometry for the navigation core: the strip's page tops and
    /// rendered sizes at the current zoom, plus the viewport, all in logical
    /// pixels. `set_geometry` re-clamps the scroll offset as a side effect.
    fn geometry(&self, viewport: Vec2) -> Geometry {
        let layout = self.layout();
        let zoom = self.zoom.factor();
        Geometry {
            page_tops: (0..layout.len()).map(|i| layout.top(i)).collect(),
            page_sizes: match &self.document {
                Some(document) => document
                    .page_sizes_points
                    .iter()
                    .map(|&(w, h)| (w * zoom, h * zoom))
                    .collect(),
                None => Vec::new(),
            },
            zoom,
            viewport: (viewport.x, viewport.y),
        }
    }

    /// Apply a zoom change, keeping whatever sits at the viewport center in
    /// place; cached bitmaps of the old zoom are dropped.
    fn apply_zoom(&mut self, factor: f32, viewport: Vec2) {
        if factor == self.zoom.factor() {
            return;
        }
        let scroll_y = self.navigator.scroll_y();
        let old_layout = self.layout();
        let page_i = old_layout.current_page(scroll_y, viewport.y);
        let fraction = if old_layout.height(page_i) > 0.0 {
            (scroll_y + viewport.y / 2.0 - old_layout.top(page_i)) / old_layout.height(page_i)
        } else {
            0.0
        }
        .clamp(0.0, 1.0);
        self.zoom = Zoom::new(factor);
        self.cache.invalidate_zoom(self.zoom.factor());
        let new_layout = self.layout();
        self.navigator.set_geometry(self.geometry(viewport));
        self.navigator
            .set_scroll(new_layout.anchor_scroll(page_i, fraction, viewport.y));
    }

    /// The page number to show in the indicator ("Page N / total"), 1-based.
    fn current_page_number(&self, viewport: Vec2) -> usize {
        let scroll_y = self.navigator.scroll_y();
        self.layout().current_page(scroll_y, viewport.y) + 1
    }

    /// Feed one frame's input — keys, wheel, pointer, click — to the app,
    /// against the CentralPanel's exact rect (the Control Bar strip above
    /// it is not part of the viewport).
    ///
    /// This runs from `ui`, not from `eframe::App::logic`: eframe also
    /// calls `logic` while the window is hidden, where egui's input still
    /// holds the last shown frame's events — keys handled there would fire
    /// again — and `logic` only saw `content_rect`, about one Control Bar
    /// taller than the visible panel, leaving a revealed Paragraph Cursor
    /// slightly below the bottom.
    fn feed_input(&mut self, ui: &egui::Ui, panel_rect: egui::Rect) {
        let viewport = panel_rect.size();

        // Fresh geometry before any event lands: the navigation core needs
        // this frame's page tops, sizes, and viewport to clamp the scroll
        // offset and to hit-test against.
        self.navigator.set_geometry(self.geometry(viewport));

        ui.input(|i| {
            // Shift is part of typing `+`/`_`/text; it must not disable
            // plain keys. Ctrl/Alt/Command mean shortcuts (Ctrl+wheel,
            // Ctrl+d/u, and keybinds).
            let unmodified = !(i.modifiers.ctrl || i.modifiers.alt || i.modifiers.command);
            if !unmodified {
                if i.modifiers.ctrl && i.smooth_scroll_delta.y != 0.0 {
                    // Ctrl+wheel zooms, anchored at the viewport center.
                    self.apply_zoom(self.zoom.wheel(i.smooth_scroll_delta.y).factor(), viewport);
                }
                // Ctrl+d / Ctrl+u scroll half a viewport. Alt/Command chords
                // stay reserved for the system and are ignored.
                if i.modifiers.ctrl && !i.modifiers.alt && !i.modifiers.command {
                    for event in &i.events {
                        if let Event::Key { key, pressed: true, .. } = event {
                            match key {
                                Key::D => self.navigator.handle(NavEvent::HalfPageDown),
                                Key::U => self.navigator.handle(NavEvent::HalfPageUp),
                                _ => {}
                            }
                        }
                    }
                }
                return;
            }

            self.navigator.handle(NavEvent::ScrollBy(-i.smooth_scroll_delta.y));

            for event in &i.events {
                if let Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    if modifiers.ctrl || modifiers.alt || modifiers.command {
                        continue;
                    }
                    match key {
                        Key::Minus | Key::OpenBracket => {
                            self.apply_zoom(self.zoom.factor() / STEP, viewport);
                        }
                        Key::Plus | Key::Equals => {
                            self.apply_zoom(self.zoom.factor() * STEP, viewport);
                        }
                        Key::Num0 => self.apply_zoom(self.zoom.reset().factor(), viewport),
                        Key::O => self.open_with_picker(),
                        Key::Q => ui
                            .ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Close),
                        // Paragraph Cursor movement along the Reading Order;
                        // the Navigator scrolls to keep it in view.
                        Key::J | Key::ArrowDown => self.navigator.handle(NavEvent::CursorDown),
                        Key::K | Key::ArrowUp => self.navigator.handle(NavEvent::CursorUp),
                        // `g` to the top of the Reading Order, `G` (shifted)
                        // to the bottom.
                        Key::G if modifiers.shift => self.navigator.handle(NavEvent::JumpBottom),
                        Key::G => self.navigator.handle(NavEvent::JumpTop),
                        Key::Escape => self.navigator.handle(NavEvent::ClearSelection),
                        _ => {}
                    }
                }
            }
        });

        // The core sees the pointer before anything is painted. Pointer
        // coordinates are panel-relative, so the core adds scroll itself
        // when hit-testing. Positions outside the panel are ignored.
        let top_left = panel_rect.min;
        let to_local = |pos: egui::Pos2| (pos.x - top_left.x, pos.y - top_left.y);
        let hover = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|pos| panel_rect.contains(*pos));
        self.navigator.handle(NavEvent::PointerMove(hover.map(to_local)));
        let click = ui
            .input(|i| {
                i.pointer
                    .primary_clicked()
                    .then(|| i.pointer.interact_pos())
                    .flatten()
            })
            .filter(|pos| panel_rect.contains(*pos));
        if let Some(pos) = click {
            self.navigator.handle(NavEvent::Click(to_local(pos)));
        }
    }
}

impl eframe::App for PdfApp {
    /// Painting: page indicator strip and the page bitmaps. Also feeds the
    /// frame's input to the core, right after the layout is known, so keys,
    /// wheel, pointer, and click all act on the same viewport geometry.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Keep the window title in step with the open Document.
        let wanted_title = self
            .document
            .as_ref()
            .map(|document| {
                format!(
                    "{} — poopdf",
                    document
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| document.path.display().to_string())
                )
            })
            .unwrap_or_else(|| "poopdf".to_string());
        if self.last_applied_title.as_deref() != Some(&wanted_title) {
            self.last_applied_title = Some(wanted_title.clone());
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Title(wanted_title));
        }

        egui::Panel::top("page_indicator").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(document) = &self.document {
                    let viewport = ui.max_rect().size();
                    ui.label(format!(
                        "Page {} / {}",
                        self.current_page_number(viewport),
                        document.page_count()
                    ));
                }
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::RED, error);
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let panel_rect = ui.max_rect();
            // Input feeding comes before the Empty State early-return so
            // `o`/`q`/zoom work with no Document open.
            self.feed_input(ui, panel_rect);

            if self.document.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("poopdf");
                        ui.label("Press `o` to choose a PDF to read");
                    });
                });
                return;
            }

            let viewport = panel_rect.size();
            let top_left = panel_rect.min;

            let view = self.navigator.view_model();
            let scroll_y = view.scroll_y;

            let document = self.document.as_ref().expect("checked above");
            let layout = self.layout();
            let zoom = self.zoom.factor();

            let painter = ui.painter().clone();
            for page_i in 0..document.page_count() {
                let page_top = layout.top(page_i);
                if page_top > scroll_y + viewport.y || page_top + layout.height(page_i) < scroll_y
                {
                    continue; // off-screen
                }
                let (w_points, h_points) = document.page_sizes_points[page_i];
                let size = Vec2::new(w_points * zoom, h_points * zoom);
                let min = egui::pos2(
                    top_left.x + (viewport.x - size.x) / 2.0,
                    top_left.y - scroll_y + page_top,
                );
                let rect = egui::Rect::from_min_size(min, size);

                // Rasterize (through the LRU bitmap cache) and draw.
                let cache_key = CacheKey::new(page_i, zoom);
                if self.cache.get(cache_key).is_none() {
                    match document.render_page(page_i, zoom) {
                        Ok(bitmap) => {
                            let image = ColorImage::from_rgba_unmultiplied(
                                [bitmap.width as usize, bitmap.height as usize],
                                &bitmap.rgba,
                            );
                            let texture = ui.ctx().load_texture(
                                format!("page-{page_i}-zoom-{}", zoom),
                                image,
                                egui::TextureOptions::LINEAR,
                            );
                            self.cache.insert(cache_key, texture, bitmap.cost_bytes());
                        }
                        Err(e) => {
                            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(60, 30, 30));
                            painter.text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                format!("Render error: {e}"),
                                egui::TextStyle::Body.resolve(ui.style()),
                                egui::Color32::WHITE,
                            );
                            continue;
                        }
                    }
                }
                if let Some(texture) = self.cache.get(cache_key) {
                    painter.image(
                        texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                    // Page edge, so white pages stay visible against the background.
                    painter.rect_stroke(
                        rect,
                        0.0,
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(90, 90, 90)),
                        egui::StrokeKind::Inside,
                    );
                }
            }

            // Highlight boxes ride the strip: screen = panel top-left +
            // (strip position - scroll). The closures capture only Copy
            // values, so mapping with them by value is fine.
            let to_screen = |r: StripRect| {
                egui::Rect::from_min_size(
                    egui::pos2(top_left.x + r.x, top_left.y + r.y - scroll_y),
                    egui::vec2(r.w, r.h),
                )
            };
            // Selection first, so the cursor and hover outlines stay on top
            // of its fill.
            if let Some(rect) = view.selected.map(to_screen) {
                painter.rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(40, 110, 240, 45),
                );
                painter.rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(40, 110, 240)),
                    egui::StrokeKind::Outside,
                );
            }
            // Hover Highlight: the Paragraph under the pointer.
            if let Some(rect) = view.hovered.map(to_screen) {
                painter.rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 150, 200)),
                    egui::StrokeKind::Outside,
                );
            }
            // Paragraph Cursor: the thickest, warmest box — where keyboard
            // navigation currently sits.
            if let Some(rect) = view.cursor.map(to_screen) {
                painter.rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 150, 0)),
                    egui::StrokeKind::Outside,
                );
            }
        });
    }
}
