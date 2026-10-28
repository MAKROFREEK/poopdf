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

//! Reading-Order smoke test against the committed sample PDF
//! (`tests/fixtures/sample.pdf`, generated uncompressed with plain Helvetica):
//!
//! - page 1: "Alpha line one wraps onto a second line right here" (2 lines),
//!   a blank line, "Beta paragraph follows after the gap", a blank line,
//!   "Chi sits between the others"
//! - page 2: "Gamma opens page two", a blank line, "Delta ends the document"
//!
//! Ignored by default because it needs the provisioned pdfium library; run
//! with `cargo test --test reading_order -- --ignored` (or the blanket
//! `cargo test -- --ignored`).

use std::path::Path;

use poopdf::document::Document;
use poopdf::pdfium_lib;

const SAMPLE_PDF: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.pdf");

#[test]
#[ignore = "needs provisioned pdfium (see module docs)"]
fn sample_pdf_yields_paragraphs_with_word_boxes_in_reading_order() {
    let pdfium = pdfium_lib::bind_pinned_pdfium().expect("pdfium provisioned");
    let document = Document::open(pdfium, Path::new(SAMPLE_PDF)).expect("sample PDF opens");
    assert_eq!(document.page_count(), 2);

    let paragraphs = document.paragraphs().expect("text extraction succeeds");

    let texts: Vec<String> = paragraphs.iter().map(|p| p.text()).collect();
    assert_eq!(
        texts,
        vec![
            "Alpha line one wraps onto a second line right here",
            "Beta paragraph follows after the gap",
            "Chi sits between the others",
            "Gamma opens page two",
            "Delta ends the document",
        ],
        "Reading Order: page by page, top to bottom"
    );

    // Pages in sequence: first three Paragraphs on page 0, last two on page 1.
    let pages: Vec<usize> = paragraphs.iter().map(|p| p.page).collect();
    assert_eq!(pages, vec![0, 0, 0, 1, 1]);

    // Every Paragraph carries Word Boxes with real bounds inside the page.
    for paragraph in &paragraphs {
        assert!(!paragraph.words.is_empty(), "Word Boxes present");
        assert!(
            paragraph.words.iter().all(|w| w.rect.w > 0.0 && w.rect.h > 0.0),
            "word boxes have size: {}",
            paragraph.text()
        );
        let page_h = document.page_sizes_points[paragraph.page].1;
        assert!(
            paragraph.rect.y >= 0.0 && paragraph.rect.y + paragraph.rect.h <= page_h,
            "paragraph box inside the page: {}",
            paragraph.text()
        );
    }

    // The wrapped first Paragraph has its two lines in Reading Order
    // (top-to-bottom) inside one box spanning both lines.
    let first = &paragraphs[0];
    assert_eq!(first.words.len(), 10, "two lines, words kept separately");
    assert_eq!(first.words[0].text, "Alpha");
    assert!(
        first.rect.h > 12.0,
        "paragraph box spans both lines, got h = {}",
        first.rect.h
    );
}
