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

//! The open Document: pdfium binding, page geometry, and page rendering.
//!
//! Pages rasterize to RGBA bitmaps at the requested zoom (logical pixels per
//! PDF point); the viewer feeds those into its texture cache.

use std::path::{Path, PathBuf};

use pdfium_render::prelude::*;

/// One rendered page: straight-alpha RGBA, row-major, top-left origin.
#[derive(Debug, Clone)]
pub struct PageBitmap {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl PageBitmap {
    /// Byte cost — what the page cache budget accounts.
    pub fn cost_bytes(&self) -> u64 {
        self.rgba.len() as u64
    }
}

pub struct Document {
    /// Where the Document was opened from.
    pub path: PathBuf,
    /// Size of every page in PDF points, in page order.
    pub page_sizes_points: Vec<(f32, f32)>,
    /// Page gaps in the vertical strip, in logical pixels at zoom 1.
    pub page_spacing_points: f32,
    document: PdfDocument<'static>,
}

impl Document {
    /// Open a PDF. `pdfium` is the leaked, process-wide binding.
    pub fn open(pdfium: &'static Pdfium, path: &Path) -> Result<Self, PdfiumError> {
        let document = pdfium.load_pdf_from_file(path, None)?;
        let pages = document.pages();
        let sizes = pages
            .iter()
            .map(|page| (page.width().value, page.height().value))
            .collect();
        Ok(Self {
            path: path.to_path_buf(),
            page_sizes_points: sizes,
            page_spacing_points: 16.0,
            document,
        })
    }

    pub fn page_count(&self) -> usize {
        self.page_sizes_points.len()
    }

    /// Render page `page` to an RGBA bitmap at the given zoom
    /// (logical pixels per PDF point).
    pub fn render_page(&self, page: usize, zoom: f32) -> Result<PageBitmap, PdfiumError> {
        let page = self.document.pages().get(page as PdfPageIndex)?;
        let target_width = ((page.width().value * zoom).round() as i32).clamp(1, 8192);
        let config = PdfRenderConfig::new()
            .set_target_width(target_width)
            .set_maximum_height(8192);
        let bitmap = page.render_with_config(&config)?;
        Ok(PageBitmap {
            width: bitmap.width() as u32,
            height: bitmap.height() as u32,
            rgba: bitmap.as_rgba_bytes(),
        })
    }
}
