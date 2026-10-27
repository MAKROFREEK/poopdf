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

//! Page layout geometry: where each page sits in the vertical document strip,
//! which page the view is on, and how to reposition the view after a zoom.
//!
//! All coordinates are logical pixels. Heights are sizes at the current zoom.

/// The vertical strip of pages: cumulative tops, one height per page.
#[derive(Debug, Clone)]
pub struct PageLayout {
    tops: Vec<f32>,
    heights: Vec<f32>,
}

impl PageLayout {
    /// Build from per-page rendered heights (at the current zoom) and the
    /// gap drawn between consecutive pages.
    pub fn new(heights: &[f32], spacing: f32) -> Self {
        let mut tops = Vec::with_capacity(heights.len());
        let mut y = 0.0;
        for &h in heights {
            tops.push(y);
            y += h + spacing;
        }
        Self {
            tops,
            heights: heights.to_vec(),
        }
    }

    pub fn len(&self) -> usize {
        self.heights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    /// Top edge y of page `page`.
    pub fn top(&self, page: usize) -> f32 {
        self.tops.get(page).copied().unwrap_or(0.0)
    }

    /// Rendered height of page `page` at the current zoom.
    pub fn height(&self, page: usize) -> f32 {
        self.heights.get(page).copied().unwrap_or(0.0)
    }

    /// Total strip height (last page without trailing spacing).
    pub fn total_height(&self) -> f32 {
        if self.is_empty() {
            0.0
        } else {
            let last = self.len() - 1;
            self.top(last) + self.height(last)
        }
    }

    /// The page the view is on: the page holding the viewport center.
    /// Scrolled above the start clamps to 0, past the end to the last page.
    pub fn current_page(&self, scroll_y: f32, viewport_height: f32) -> usize {
        if self.is_empty() {
            return 0;
        }
        let center = (scroll_y + viewport_height / 2.0).max(0.0);
        // Last page whose top is at or above the center point.
        let page = self
            .tops
            .iter()
            .rposition(|&top| top <= center)
            .unwrap_or(0);
        page.min(self.len() - 1)
    }

    /// The scroll offset that puts `fraction` of page `page` at the viewport
    /// center — used to keep the same content in view across a zoom change.
    /// Clamped to the scrollable range.
    pub fn anchor_scroll(&self, page: usize, fraction: f32, viewport_height: f32) -> f32 {
        let content_y = self.top(page) + fraction * self.height(page);
        self.clamp_scroll(content_y - viewport_height / 2.0, viewport_height)
    }

    /// Clamp a raw scroll offset into the scrollable range.
    pub fn clamp_scroll(&self, scroll_y: f32, viewport_height: f32) -> f32 {
        let max = (self.total_height() - viewport_height).max(0.0);
        scroll_y.clamp(0.0, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three pages of heights 100, 200, 300 with spacing 10.
    fn layout() -> PageLayout {
        PageLayout::new(&[100.0, 200.0, 300.0], 10.0)
    }

    #[test]
    fn tops_are_cumulative_with_spacing() {
        let l = layout();
        assert_eq!(l.top(0), 0.0);
        assert_eq!(l.top(1), 110.0); // 100 + 10
        assert_eq!(l.top(2), 320.0); // 110 + 200 + 10
        assert_eq!(l.total_height(), 620.0); // no trailing spacing
    }

    #[test]
    fn current_page_is_the_page_holding_the_viewport_center() {
        let l = layout();
        // scroll 40, viewport 100 → center at 90 → page 0
        assert_eq!(l.current_page(40.0, 100.0), 0);
        // scroll 120, viewport 100 → center 170 → page 1 (110..310)
        assert_eq!(l.current_page(120.0, 100.0), 1);
        // scroll 400, viewport 100 → center 450 → page 2 (320..620)
        assert_eq!(l.current_page(400.0, 100.0), 2);
    }

    #[test]
    fn current_page_clamps_beyond_both_ends() {
        let l = layout();
        assert_eq!(l.current_page(-500.0, 100.0), 0);
        assert_eq!(l.current_page(10_000.0, 100.0), 2);
    }

    #[test]
    fn empty_layout_is_always_page_zero() {
        let l = PageLayout::new(&[], 10.0);
        assert_eq!(l.current_page(50.0, 100.0), 0);
    }

    #[test]
    fn one_page_taller_than_the_viewport_stays_that_page() {
        let l = PageLayout::new(&[1_000.0], 10.0);
        assert_eq!(l.current_page(0.0, 100.0), 0);
        assert_eq!(l.current_page(900.0, 100.0), 0);
    }

    #[test]
    fn anchor_scroll_puts_the_page_fraction_at_viewport_center() {
        let l = layout();
        // Fraction 0.0 of page 1 (top 110) centered in a 100 viewport:
        // 110 - 50 = 60
        assert!((l.anchor_scroll(1, 0.0, 100.0) - 60.0).abs() < 1e-4);
    }

    #[test]
    fn anchor_scroll_clamps_into_the_scrollable_range() {
        let l = layout();
        // Centering the bottom of the last page exceeds the range: clamped
        // to total - viewport = 620 - 100 = 520.
        assert!((l.anchor_scroll(2, 1.0, 100.0) - 520.0).abs() < 1e-4);
    }

    #[test]
    fn anchor_scroll_when_document_shorter_than_viewport() {
        let l = PageLayout::new(&[50.0], 10.0);
        assert_eq!(l.anchor_scroll(0, 0.5, 100.0), 0.0);
    }

    #[test]
    fn clamp_scroll_bounds_the_offset() {
        let l = layout();
        assert_eq!(l.clamp_scroll(-10.0, 100.0), 0.0);
        assert_eq!(l.clamp_scroll(10_000.0, 100.0), 520.0);
        assert_eq!(l.clamp_scroll(200.0, 100.0), 200.0);
    }
}
