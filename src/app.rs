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
use crate::page_cache::{CacheKey, DEFAULT_BUDGET_BYTES, PageCache};
use crate::pdfium_lib;
use crate::view::PageLayout;
use crate::zoom::{STEP, Zoom};

pub struct PdfApp {
    pdfium: &'static pdfium_render::prelude::Pdfium,
    document: Option<Document>,
    error: Option<String>,
    zoom: Zoom,
    scroll_y: f32,
    cache: PageCache<TextureHandle>,
    /// File name currently shown in the window title.
    titled: Option<String>,
}

impl PdfApp {
    /// Provision pdfium once, then create the app. A CLI path opens the
    /// Document immediately; on failure we show the error in-app.
    pub fn new(start_path: Option<String>) -> Result<Self, String> {
        let pdfium = pdfium_lib::bind_pinned_pdfium().map_err(|e| e.to_string())?;
        let mut app = Self {
            pdfium,
            document: None,
            error: None,
            zoom: Zoom::default(),
            scroll_y: 0.0,
            cache: PageCache::new(DEFAULT_BUDGET_BYTES),
            titled: None,
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
                self.document = Some(document);
                self.error = None;
                self.zoom = Zoom::default();
                self.scroll_y = 0.0;
                self.cache.invalidate_zoom(self.zoom.factor());
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

    /// Apply a zoom change, keeping whatever sits at the viewport center in
    /// place; cached bitmaps of the old zoom are dropped.
    fn apply_zoom(&mut self, factor: f32, viewport: Vec2) {
        if factor == self.zoom.factor() {
            return;
        }
        let old_layout = self.layout();
        let page_i = old_layout.current_page(self.scroll_y, viewport.y);
        let fraction = if old_layout.height(page_i) > 0.0 {
            (self.scroll_y + viewport.y / 2.0 - old_layout.top(page_i)) / old_layout.height(page_i)
        } else {
            0.0
        }
        .clamp(0.0, 1.0);
        self.zoom = Zoom::new(factor);
        self.cache.invalidate_zoom(self.zoom.factor());
        let new_layout = self.layout();
        self.scroll_y = new_layout.anchor_scroll(page_i, fraction, viewport.y);
    }

    /// The page number to show in the indicator ("Page N / total"), 1-based.
    fn current_page_number(&self, viewport: Vec2) -> usize {
        self.layout().current_page(self.scroll_y, viewport.y) + 1
    }

    fn viewport_size(ctx: &egui::Context) -> Vec2 {
        ctx.input(|i| i.content_rect().size())
    }
}

impl eframe::App for PdfApp {
    /// Input handling: keys and wheel. No painting here.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let viewport = Self::viewport_size(ctx);

        ctx.input(|i| {
            for event in &i.events {
                if let Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    if *modifiers != egui::Modifiers::default() {
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
                        Key::Q => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                        _ => {}
                    }
                }
            }

            if i.modifiers.ctrl && i.smooth_scroll_delta.y != 0.0 {
                // Ctrl+wheel zooms, anchored at the viewport center.
                self.apply_zoom(self.zoom.wheel(i.smooth_scroll_delta.y).factor(), viewport);
            } else {
                self.scroll_y -= i.smooth_scroll_delta.y;
            }

            for event in &i.events {
                if let Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    if *modifiers != egui::Modifiers::default() {
                        continue;
                    }
                    let step = viewport.y * 0.5;
                    match key {
                        Key::ArrowDown => self.scroll_y += step,
                        Key::ArrowUp => self.scroll_y -= step,
                        _ => {}
                    }
                }
            }
        });
    }

    /// Painting: page indicator strip and the page bitmaps.
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
        if self.titled.as_deref() != Some(&wanted_title) {
            self.titled = Some(wanted_title.clone());
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
            let Some(document) = &self.document else {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("poopdf");
                        ui.label("Press `o` to open a PDF");
                    });
                });
                return;
            };
            let viewport = ui.max_rect().size();
            let layout = self.layout();
            let zoom = self.zoom.factor();
            let top_left = ui.max_rect().min;

            self.scroll_y = layout.clamp_scroll(self.scroll_y, viewport.y);

            let painter = ui.painter().clone();
            for page_i in 0..document.page_count() {
                let page_top = layout.top(page_i);
                if page_top > self.scroll_y + viewport.y
                    || page_top + layout.height(page_i) < self.scroll_y
                {
                    continue; // off-screen
                }
                let (w_points, h_points) = document.page_sizes_points[page_i];
                let size = Vec2::new(w_points * zoom, h_points * zoom);
                let min = egui::pos2(
                    top_left.x + (viewport.x - size.x) / 2.0,
                    top_left.y - self.scroll_y + page_top,
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
                            let texture = ctx_of(ui).load_texture(
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
        });
    }
}

fn ctx_of(ui: &mut egui::Ui) -> egui::Context {
    ui.ctx().clone()
}
