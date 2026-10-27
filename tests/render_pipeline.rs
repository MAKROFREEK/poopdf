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

//! Full render-pipeline checks against the real pinned pdfium library.
//! Ignored by default: run with `cargo test -- --ignored` on a machine that
//! has provisioned pdfium (`cargo run` once) and a test PDF.

use std::path::PathBuf;

use poopdf::document::Document;
use poopdf::pdfium_lib;

const POOPDF_TEST_PDF: &str = "POOPDF_TEST_PDF";

fn test_pdf() -> Option<PathBuf> {
    std::env::var(POOPDF_TEST_PDF)
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
}

#[test]
#[ignore = "needs provisioned pdfium and POOPDF_TEST_PDF (see module docs)"]
fn document_opens_and_reports_page_sizes() {
    let Some(path) = test_pdf() else {
        panic!("set {POOPDF_TEST_PDF} to a real PDF to run this");
    };
    let pdfium = pdfium_lib::bind_pinned_pdfium().expect("pdfium provisioned");
    let document = Document::open(pdfium, &path).expect("opens");
    assert_eq!(document.page_count(), 1);
    let (w, h) = document.page_sizes_points[0];
    assert!((w - 612.0).abs() < 0.5, "letter width in points, got {w}");
    assert!((h - 792.0).abs() < 0.5, "letter height in points, got {h}");
}

#[test]
#[ignore = "needs provisioned pdfium and POOPDF_TEST_PDF (see module docs)"]
fn renders_first_page_at_correct_zoom() {
    let Some(path) = test_pdf() else {
        panic!("set {POOPDF_TEST_PDF} to a real PDF to run this");
    };
    let pdfium = pdfium_lib::bind_pinned_pdfium().expect("pdfium provisioned");
    let document = Document::open(pdfium, &path).expect("opens");

    let zoom = 2.0;
    let bitmap = document.render_page(0, zoom).expect("renders");
    assert_eq!(bitmap.width, (612.0 * zoom).round() as u32);
    assert_eq!(bitmap.height, (792.0 * zoom).round() as u32);
    assert_eq!(
        bitmap.rgba.len() as u64,
        (bitmap.width * bitmap.height * 4) as u64
    );

    // A text page is not blank: inside the first line's band there must be
    // dark ("ink") pixels. ("Hello poopdf" at 24pt, baseline 700pt from the
    // bottom-left origin: near the top of the bitmap.)
    let scale = bitmap.width as f32 / 612.0;
    let x0 = (72.0 * scale) as usize;
    let x1 = (260.0 * scale) as usize;
    let y0 = ((792.0 - 706.0) * scale) as usize;
    let y1 = y0 + (30.0 * scale) as usize;
    let stride = bitmap.width as usize * 4;
    let ink = (y0..y1).any(|y| {
        (x0..x1).any(|x| {
            let p = y * stride + x * 4;
            bitmap.rgba[p] < 128 && bitmap.rgba[p + 1] < 128 && bitmap.rgba[p + 2] < 128
        })
    });
    assert!(ink, "expected dark text pixels in the first line's band");
}
