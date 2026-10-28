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

//! The platform-free navigation core: a state machine taking events
//! (key presses, pointer hover/click) and emitting a view model (Paragraph
//! Cursor / Hover Highlight / selection geometry in strip coordinates, plus
//! view state). No egui or pdfium types cross this seam — the app converts
//! input into [`Event`]s and paints the [`ViewModel`].
//!
//! Strip coordinates are logical pixels with origin at the top-left of the
//! first page, y growing down, *before* scroll; painting translates by
//! `scroll_y`. Pointer events carry content-panel coordinates (the app
//! subtracts the panel origin); the core adds scroll itself so a stationary
//! pointer keeps tracking the paragraph underneath it as the view moves.

use crate::paragraphs::Paragraph;

/// A rectangle in strip coordinates: logical pixels, origin top-left of the
/// strip, y growing down (pre-scroll).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl StripRect {
    /// The point is inside, edges included.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

/// View geometry the core needs to place Paragraphs in the strip. The app
/// rebuilds it whenever the layout, zoom, or viewport changes and pushes it
/// in via [`Navigator::set_geometry`].
#[derive(Debug, Clone)]
pub struct Geometry {
    /// Strip-space y of each page's top edge, logical px at the current zoom.
    pub page_tops: Vec<f32>,
    /// Rendered `(width, height)` of each page, logical px at the current zoom.
    pub page_sizes: Vec<(f32, f32)>,
    /// Logical pixels per PDF point.
    pub zoom: f32,
    /// Content viewport `(width, height)` in logical pixels.
    pub viewport: (f32, f32),
}

impl Default for Geometry {
    fn default() -> Self {
        // zoom 1.0, not f32::default's 0.0 — geometry without pages must
        // still be usable for conversions.
        Self {
            page_tops: Vec::new(),
            page_sizes: Vec::new(),
            zoom: 1.0,
            viewport: (0.0, 0.0),
        }
    }
}

/// What the viewer paints: highlight geometry and view state, recomputed
/// from core state by [`Navigator::view_model`] on demand.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ViewModel {
    /// Current scroll offset in strip pixels.
    pub scroll_y: f32,
    /// Outline box of the Paragraph Cursor.
    pub cursor: Option<StripRect>,
    /// Outline box of the Hover Highlight.
    pub hovered: Option<StripRect>,
    /// Outline box of the selection made by clicking.
    pub selected: Option<StripRect>,
    pub cursor_index: Option<usize>,
    pub hovered_index: Option<usize>,
    pub selected_index: Option<usize>,
}

/// Events the core accepts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// `j` / `↓`: move the Paragraph Cursor down one Paragraph.
    CursorDown,
    /// `k` / `↑`: move the Paragraph Cursor up one Paragraph.
    CursorUp,
    /// `g`: Paragraph Cursor to the first Paragraph, view to the top.
    JumpTop,
    /// `G`: Paragraph Cursor to the last Paragraph, view to the bottom.
    JumpBottom,
    /// `Ctrl+d`: scroll half a viewport down; the cursor does not move.
    HalfPageDown,
    /// `Ctrl+u`: scroll half a viewport up; the cursor does not move.
    HalfPageUp,
    /// Plain wheel scrolling: move the view down by this many strip pixels.
    ScrollBy(f32),
    /// Pointer moved to content-panel coordinates, or [`None`] when it left
    /// the content area.
    PointerMove(Option<(f32, f32)>),
    /// Click at content-panel coordinates: select the Paragraph under it.
    Click((f32, f32)),
    /// `Esc`: clear the selection.
    ClearSelection,
}

/// The navigation state machine: Paragraph Cursor, Hover Highlight,
/// selection, and scroll, over a Document's Paragraphs in Reading Order.
pub struct Navigator {
    geometry: Geometry,
    paragraphs: Vec<Paragraph>,
    cursor: Option<usize>,
    hovered: Option<usize>,
    selected: Option<usize>,
    scroll_y: f32,
    /// Last known pointer position in content-panel coordinates.
    pointer: Option<(f32, f32)>,
}

impl Navigator {
    pub fn new() -> Self {
        Self {
            geometry: Geometry::default(),
            paragraphs: Vec::new(),
            cursor: None,
            hovered: None,
            selected: None,
            scroll_y: 0.0,
            pointer: None,
        }
    }

    /// Replace the Paragraphs (a Document was opened): the Paragraph Cursor
    /// starts on the first Paragraph, selection clears, view jumps to top.
    pub fn set_paragraphs(&mut self, paragraphs: Vec<Paragraph>) {
        self.cursor = if paragraphs.is_empty() { None } else { Some(0) };
        self.selected = None;
        self.scroll_y = 0.0;
        self.paragraphs = paragraphs;
        self.refresh_hover();
    }

    /// Update view geometry (layout/zoom/viewport changed); the scroll
    /// offset is re-clamped into the new range.
    pub fn set_geometry(&mut self, geometry: Geometry) {
        self.geometry = geometry;
        self.scroll_y = self.clamp_scroll(self.scroll_y);
        self.refresh_hover();
    }

    /// Current scroll offset in strip pixels — the view's single source of
    /// truth; painting and the page indicator read it every frame.
    pub fn scroll_y(&self) -> f32 {
        self.scroll_y
    }

    /// Force the view to an absolute strip offset (zoom anchoring); clamped.
    pub fn set_scroll(&mut self, scroll_y: f32) {
        self.scroll_y = self.clamp_scroll(scroll_y);
        self.refresh_hover();
    }

    /// Feed one event; state (cursor, hover, selection, scroll) advances.
    pub fn handle(&mut self, event: Event) {
        match event {
            Event::CursorDown => {
                self.move_cursor(1);
                self.reveal_cursor();
            }
            Event::CursorUp => {
                self.move_cursor(-1);
                self.reveal_cursor();
            }
            Event::JumpTop => {
                if !self.paragraphs.is_empty() {
                    self.cursor = Some(0);
                }
                // Absolute top of the document, so page margins and headers
                // above the first Paragraph are visible too.
                self.scroll_y = self.clamp_scroll(0.0);
                self.refresh_hover();
            }
            Event::JumpBottom => {
                if !self.paragraphs.is_empty() {
                    let last = self.paragraphs.len() - 1;
                    self.cursor = Some(last);
                }
                // Bottom of the document — unless that would leave the
                // Paragraph Cursor (on the last Paragraph) out of view.
                self.scroll_y = self.clamp_scroll(f32::MAX);
                self.reveal_cursor();
                self.refresh_hover();
            }
            Event::HalfPageDown => {
                let half_viewport = self.geometry.viewport.1 / 2.0;
                self.scroll_y = self.clamp_scroll(self.scroll_y + half_viewport);
                self.refresh_hover();
            }
            Event::HalfPageUp => {
                let half_viewport = self.geometry.viewport.1 / 2.0;
                self.scroll_y = self.clamp_scroll(self.scroll_y - half_viewport);
                self.refresh_hover();
            }
            Event::ScrollBy(delta) => {
                self.scroll_y = self.clamp_scroll(self.scroll_y + delta);
                self.refresh_hover();
            }
            Event::PointerMove(pos) => {
                self.pointer = pos;
                self.refresh_hover();
            }
            Event::Click(pos) => {
                // A click implies the pointer is here (touch and drag-free
                // clicks arrive without a preceding hover event).
                self.pointer = Some(pos);
                self.selected = self.hit_test(pos);
                self.refresh_hover();
            }
            Event::ClearSelection => {
                self.selected = None;
            }
        }
    }

    /// Move the Paragraph Cursor by `delta` Paragraphs, clamped at the ends
    /// of the Reading Order (an empty Document has no cursor to move).
    fn move_cursor(&mut self, delta: isize) {
        let Some(cursor) = self.cursor else { return };
        if self.paragraphs.is_empty() {
            self.cursor = None;
            return;
        }
        let last = self.paragraphs.len() - 1;
        self.cursor = Some((cursor as isize + delta).clamp(0, last as isize) as usize);
    }

    /// Scroll just enough to bring the Paragraph Cursor fully into view.
    /// Without this, moving the cursor past the viewport bottom would drag
    /// the outline off screen and the user would lose track of it.
    fn reveal_cursor(&mut self) {
        let Some(index) = self.cursor else { return };
        let Some(rect) = self.strip_rect(index) else { return };
        let viewport_h = self.geometry.viewport.1;
        let target = if rect.y + rect.h > self.scroll_y + viewport_h {
            // Cursor bottom hangs below the viewport bottom.
            rect.y + rect.h - viewport_h
        } else if rect.y < self.scroll_y {
            // Cursor top sits above the viewport top.
            rect.y
        } else {
            return;
        };
        self.scroll_y = self.clamp_scroll(target);
        self.refresh_hover();
    }

    /// The current view model: highlight geometry + view state.
    pub fn view_model(&self) -> ViewModel {
        ViewModel {
            scroll_y: self.scroll_y,
            cursor: self.cursor.and_then(|i| self.strip_rect(i)),
            hovered: self.hovered.and_then(|i| self.strip_rect(i)),
            selected: self.selected.and_then(|i| self.strip_rect(i)),
            cursor_index: self.cursor,
            hovered_index: self.hovered,
            selected_index: self.selected,
        }
    }

    /// Strip-space rectangle of a Paragraph at the current zoom, or [`None`]
    /// before geometry is set.
    fn strip_rect(&self, index: usize) -> Option<StripRect> {
        let paragraph = self.paragraphs.get(index)?;
        let &top = self.geometry.page_tops.get(paragraph.page)?;
        let &(page_w, _) = self.geometry.page_sizes.get(paragraph.page)?;
        let zoom = self.geometry.zoom;
        Some(StripRect {
            x: (self.geometry.viewport.0 - page_w) / 2.0 + paragraph.rect.x * zoom,
            y: top + paragraph.rect.y * zoom,
            w: paragraph.rect.w * zoom,
            h: paragraph.rect.h * zoom,
        })
    }

    /// Largest scroll offset for the current geometry (viewport bottom at
    /// the strip bottom), never negative.
    fn clamp_scroll(&self, scroll_y: f32) -> f32 {
        let total = match (self.geometry.page_tops.last(), self.geometry.page_sizes.last()) {
            (Some(&top), Some(&(_, h))) => top + h,
            _ => 0.0,
        };
        let max = (total - self.geometry.viewport.1).max(0.0);
        scroll_y.clamp(0.0, max)
    }

    /// Recompute the Hover Highlight from the stored pointer position, so it
    /// stays glued to the pointer across scroll and geometry changes.
    fn refresh_hover(&mut self) {
        self.hovered = self.pointer.and_then(|pos| self.hit_test(pos));
    }

    /// The Paragraph under a content-panel coordinate, if any. The pointer
    /// is panel-space, so the scroll offset is added back to reach strip
    /// space; the page is located first, then the Paragraph whose strip
    /// rectangle contains the point.
    fn hit_test(&self, pos: (f32, f32)) -> Option<usize> {
        let strip = (pos.0, pos.1 + self.scroll_y);
        let page = self
            .geometry
            .page_tops
            .iter()
            .zip(&self.geometry.page_sizes)
            .position(|(&top, &(_, h))| strip.1 >= top && strip.1 < top + h)?;
        self.paragraphs
            .iter()
            .enumerate()
            .find(|(index, paragraph)| {
                paragraph.page == page
                    && self
                        .strip_rect(*index)
                        .is_some_and(|rect| rect.contains(strip.0, strip.1))
            })
            .map(|(index, _)| index)
    }
}

impl Default for Navigator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paragraphs::{Rect, WordBox};

    /// A one-word Paragraph whose bounds are (x, y, 200, 20) page points.
    fn paragraph(page: usize, x: f32, y: f32) -> Paragraph {
        let rect = Rect {
            x,
            y,
            w: 200.0,
            h: 20.0,
        };
        Paragraph {
            page,
            words: vec![WordBox {
                text: "word".to_string(),
                rect,
            }],
            rect,
        }
    }

    /// Two letter pages: 612x792pt rendered 1:1, tops 0 and 808 (792 + 16
    /// spacing), viewport 800x600 → each page sits at strip x = (800-612)/2
    /// = 94.
    fn geometry() -> Geometry {
        Geometry {
            page_tops: vec![0.0, 808.0],
            page_sizes: vec![(612.0, 792.0), (612.0, 792.0)],
            zoom: 1.0,
            viewport: (800.0, 600.0),
        }
    }

    /// A Navigator over `paragraphs` with [`geometry`] applied.
    fn navigator_with(paragraphs: Vec<Paragraph>) -> Navigator {
        let mut navigator = Navigator::new();
        navigator.set_geometry(geometry());
        navigator.set_paragraphs(paragraphs);
        navigator
    }

    #[test]
    fn cursor_starts_on_the_first_paragraph_with_its_geometry() {
        let navigator = navigator_with(vec![paragraph(0, 72.0, 100.0), paragraph(0, 72.0, 400.0)]);

        let view = navigator.view_model();

        assert_eq!(view.cursor_index, Some(0));
        // Page left edge is strip x=94, so 72pt in lands at 166.
        assert_eq!(
            view.cursor,
            Some(StripRect {
                x: 166.0,
                y: 100.0,
                w: 200.0,
                h: 20.0
            })
        );
        assert_eq!(view.scroll_y, 0.0);
        assert_eq!(view.hovered, None);
        assert_eq!(view.selected, None);
    }

    #[test]
    fn cursor_down_and_up_walk_the_paragraphs_and_stop_at_both_ends() {
        let mut navigator = navigator_with(vec![
            paragraph(0, 72.0, 100.0),
            paragraph(0, 72.0, 400.0),
            paragraph(1, 72.0, 100.0),
        ]);

        navigator.handle(Event::CursorDown);
        assert_eq!(navigator.view_model().cursor_index, Some(1));

        navigator.handle(Event::CursorDown);
        assert_eq!(navigator.view_model().cursor_index, Some(2));

        // Past the last Paragraph: stays put.
        navigator.handle(Event::CursorDown);
        assert_eq!(navigator.view_model().cursor_index, Some(2));

        navigator.handle(Event::CursorUp);
        assert_eq!(navigator.view_model().cursor_index, Some(1));

        // Above the first Paragraph: stays put.
        navigator.handle(Event::CursorUp);
        navigator.handle(Event::CursorUp);
        assert_eq!(navigator.view_model().cursor_index, Some(0));
    }

    #[test]
    fn jumping_to_the_bottom_and_back_to_the_top_moves_cursor_and_view() {
        let mut navigator = navigator_with(vec![
            paragraph(0, 72.0, 100.0),
            paragraph(0, 72.0, 400.0),
            paragraph(1, 72.0, 400.0),
        ]);

        navigator.handle(Event::JumpBottom);
        let view = navigator.view_model();
        assert_eq!(view.cursor_index, Some(2));
        // Strip height = 808 (page 1 top) + 792 = 1600; viewport 600 →
        // the view rests at the bottom of the document.
        assert_eq!(view.scroll_y, 1000.0);

        navigator.handle(Event::JumpTop);
        let view = navigator.view_model();
        assert_eq!(view.cursor_index, Some(0));
        assert_eq!(view.scroll_y, 0.0);
    }

    #[test]
    fn half_page_scrolls_half_a_viewport_and_leaves_the_cursor_alone() {
        let mut navigator = navigator_with(vec![
            paragraph(0, 72.0, 100.0),
            paragraph(0, 72.0, 400.0),
            paragraph(1, 72.0, 400.0),
        ]);

        navigator.handle(Event::HalfPageDown);
        let view = navigator.view_model();
        assert_eq!(view.scroll_y, 300.0, "half of the 600px viewport");
        assert_eq!(view.cursor_index, Some(0), "cursor does not move");

        navigator.handle(Event::HalfPageDown);
        assert_eq!(navigator.view_model().scroll_y, 600.0);

        navigator.handle(Event::HalfPageUp);
        assert_eq!(navigator.view_model().scroll_y, 300.0);

        // Already at the top: clamped instead of going negative.
        navigator.handle(Event::HalfPageUp);
        navigator.handle(Event::HalfPageUp);
        assert_eq!(navigator.view_model().scroll_y, 0.0);

        // And clamped at the bottom of the strip (1600 - 600 = 1000).
        navigator.handle(Event::JumpBottom);
        navigator.handle(Event::HalfPageDown);
        assert_eq!(navigator.view_model().scroll_y, 1000.0);
    }

    #[test]
    fn wheel_scrolls_the_view_and_clamps_at_both_ends() {
        let mut navigator = navigator_with(vec![
            paragraph(0, 72.0, 100.0),
            paragraph(1, 72.0, 400.0),
        ]);

        navigator.handle(Event::ScrollBy(250.0));
        assert_eq!(navigator.view_model().scroll_y, 250.0);

        navigator.handle(Event::ScrollBy(-1000.0));
        assert_eq!(navigator.view_model().scroll_y, 0.0);

        navigator.handle(Event::ScrollBy(999_999.0));
        assert_eq!(navigator.view_model().scroll_y, 1000.0);
    }

    #[test]
    fn clicking_a_paragraph_selects_it_and_escaping_clears_the_selection() {
        let mut navigator = navigator_with(vec![
            paragraph(0, 72.0, 100.0),
            paragraph(0, 72.0, 400.0),
        ]);

        navigator.handle(Event::Click((200.0, 410.0)));
        let view = navigator.view_model();
        assert_eq!(view.selected_index, Some(1));
        assert_eq!(
            view.selected,
            Some(StripRect {
                x: 166.0,
                y: 400.0,
                w: 200.0,
                h: 20.0
            })
        );
        // Selecting does not steal the Paragraph Cursor.
        assert_eq!(view.cursor_index, Some(0));

        navigator.handle(Event::ClearSelection);
        assert_eq!(navigator.view_model().selected_index, None);
    }

    #[test]
    fn clicking_empty_space_clears_the_selection() {
        let mut navigator = navigator_with(vec![
            paragraph(0, 72.0, 100.0),
            paragraph(0, 72.0, 400.0),
        ]);

        navigator.handle(Event::Click((200.0, 410.0)));
        assert_eq!(navigator.view_model().selected_index, Some(1));

        // Between the Paragraphs: the click dismisses the selection.
        navigator.handle(Event::Click((200.0, 250.0)));
        assert_eq!(navigator.view_model().selected_index, None);
    }

    #[test]
    fn moving_the_pointer_over_a_paragraph_hovers_it_and_leaving_clears_it() {
        // Paragraph 0 lands at strip x 166..366, y 100..120 (page left edge
        // is strip x = 94); paragraph 1 at y 400..420.
        let mut navigator = navigator_with(vec![
            paragraph(0, 72.0, 100.0),
            paragraph(0, 72.0, 400.0),
        ]);

        navigator.handle(Event::PointerMove(Some((200.0, 110.0))));
        let view = navigator.view_model();
        assert_eq!(view.hovered_index, Some(0));
        assert_eq!(
            view.hovered,
            Some(StripRect {
                x: 166.0,
                y: 100.0,
                w: 200.0,
                h: 20.0
            })
        );

        // Between the two Paragraphs: nothing to hover.
        navigator.handle(Event::PointerMove(Some((200.0, 250.0))));
        assert_eq!(navigator.view_model().hovered_index, None);

        // Pointer left the content area entirely.
        navigator.handle(Event::PointerMove(None));
        assert_eq!(navigator.view_model().hovered_index, None);
    }

    #[test]
    fn the_hover_highlight_stays_with_the_paragraph_under_a_stationary_pointer_as_the_view_scrolls() {
        let mut navigator = navigator_with(vec![
            paragraph(0, 72.0, 100.0),
            paragraph(0, 72.0, 700.0),
        ]);
        let pointer = Some((200.0, 400.0));

        // Scroll 300 down: strip y = 400 + 300 = 700 sits on paragraph 1.
        navigator.handle(Event::ScrollBy(300.0));
        navigator.handle(Event::PointerMove(pointer));
        assert_eq!(navigator.view_model().hovered_index, Some(1));

        // Scroll back: the pointer now sits over empty strip (y = 400).
        navigator.handle(Event::ScrollBy(-300.0));
        assert_eq!(navigator.view_model().hovered_index, None);

        // And scrolling down again re-finds the same Paragraph.
        navigator.handle(Event::ScrollBy(300.0));
        assert_eq!(navigator.view_model().hovered_index, Some(1));
    }

    #[test]
    fn moving_the_cursor_below_the_viewport_scrolls_to_reveal_it() {
        // Viewport 600 tall; paragraphs at y=100 (visible) and y=1200 on
        // page 1 (strip y = 808 + 1200... page 1 rect y=400 → strip 1208).
        let mut navigator = navigator_with(vec![paragraph(0, 72.0, 100.0), paragraph(1, 72.0, 400.0)]);
        assert_eq!(navigator.view_model().scroll_y, 0.0);

        navigator.handle(Event::CursorDown);

        let view = navigator.view_model();
        // Cursor strip y = 808 + 400 = 1208, bottom 1228; the view must
        // move down so the cursor is inside the 600px viewport.
        assert!(
            view.scroll_y <= 1208.0 && view.scroll_y + 600.0 >= 1228.0,
            "cursor must be visible, scroll_y = {}",
            view.scroll_y
        );
        assert!(view.scroll_y > 0.0, "view had to scroll down");
    }
}
