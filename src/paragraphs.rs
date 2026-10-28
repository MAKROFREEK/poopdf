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

//! Paragraph geometry and the platform-free grouping rules: character boxes
//! → Word Boxes → lines → Paragraphs in Reading Order.
//!
//! All coordinates are page points with a top-left origin and y growing down
//! (the viewer's painted orientation). The pdfium adapter flips PDF space
//! (bottom-left origin) before handing character boxes to
//! [`group_into_paragraphs`].

/// A rectangle in page points, top-left origin, y growing down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    /// The point is inside, edges included.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }

    /// Smallest rectangle covering both, for accumulating word/paragraph bounds.
    pub fn union(self, other: Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.w).max(other.x + other.w);
        let bottom = (self.y + self.h).max(other.y + other.h);
        Rect {
            x,
            y,
            w: right - x,
            h: bottom - y,
        }
    }
}

/// Two vertical spans sit on one visual line when they overlap by most of
/// the smaller span's height: a shifted baseline (sub/superscript, mid-line
/// font change) stays on its line, while the slight top/bottom spill of
/// adjacent physical lines (box height > line pitch) does not glue them.
fn one_visual_line(a_y: f32, a_h: f32, b_y: f32, b_h: f32) -> bool {
    let overlap = (a_y + a_h).min(b_y + b_h) - a_y.max(b_y);
    overlap >= 0.5 * a_h.min(b_h)
}

/// One extracted character and its box, as the platform layer reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct CharBox {
    pub ch: char,
    pub rect: Rect,
}

/// The position and size of one word inside a Paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct WordBox {
    pub text: String,
    pub rect: Rect,
}

/// A contiguous run of document text treated as one reading unit, in page
/// coordinates: its Word Boxes in reading sequence plus its own bounds.
#[derive(Debug, Clone, PartialEq)]
pub struct Paragraph {
    /// Index of the page this Paragraph sits on.
    pub page: usize,
    pub words: Vec<WordBox>,
    pub rect: Rect,
}

impl Paragraph {
    /// The Paragraph's text, words joined by single spaces.
    pub fn text(&self) -> String {
        self.words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Group one page's extracted characters into Paragraphs in Reading Order.
///
/// `page` is the page index the characters belong to, stamped onto every
/// Paragraph produced.
pub fn group_into_paragraphs(page: usize, chars: &[CharBox]) -> Vec<Paragraph> {
    // Current extent: whitespace separates Word Boxes; everything on the page
    // becomes one Paragraph. Line and paragraph breaks arrive in later slices.
    let mut words: Vec<WordBox> = Vec::new();
    let mut current: Option<WordBox> = None;
    for c in chars {
        // A new line also starts a new word: no whitespace sits between the
        // last glyph of one line and the first of the next, and appending
        // would glue them together.
        let starts_new_word = c.ch.is_whitespace()
            || current.as_ref().is_some_and(|word| {
                // Not on the word in progress's visual line = new word.
                !one_visual_line(c.rect.y, c.rect.h, word.rect.y, word.rect.h)
            });
        if starts_new_word && let Some(word) = current.take() {
            words.push(word);
        }
        if c.ch.is_whitespace() {
            continue;
        }
        match current.as_mut() {
            Some(word) => {
                word.text.push(c.ch);
                word.rect = word.rect.union(c.rect);
            }
            None => {
                current = Some(WordBox {
                    text: c.ch.to_string(),
                    rect: c.rect,
                });
            }
        }
    }
    if let Some(word) = current.take() {
        words.push(word);
    }

    if words.is_empty() {
        return Vec::new();
    }
    // Reading Order: lines top-to-bottom, words left-to-right within a line —
    // never the order the platform layer happened to report characters in.
    // Words are first marshalled into visual lines by vertical overlap, so
    // same-line glyphs with shifted baselines (sub/superscripts, larger
    // leading in mid-line runs) stay on their line; each line then sorts by x.
    let mut by_y = words;
    by_y.sort_by(|a, b| {
        a.rect
            .y
            .partial_cmp(&b.rect.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.rect
                    .x
                    .partial_cmp(&b.rect.x)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    let mut lines: Vec<Vec<WordBox>> = Vec::new();
    for word in by_y {
        // A word joins the open line when it stays on that line's span.
        let on_current_line = lines.last().is_some_and(|line| {
            let top = line
                .iter()
                .map(|w| w.rect.y)
                .fold(f32::INFINITY, f32::min);
            let bottom = line
                .iter()
                .map(|w| w.rect.y + w.rect.h)
                .fold(f32::NEG_INFINITY, f32::max);
            one_visual_line(word.rect.y, word.rect.h, top, bottom - top)
        });
        if !on_current_line {
            lines.push(Vec::new());
        }
        lines.last_mut().expect("just pushed").push(word);
    }
    for line in lines.iter_mut() {
        line.sort_by(|a, b| {
            a.rect
                .x
                .partial_cmp(&b.rect.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    let words: Vec<WordBox> = lines.into_iter().flatten().collect();

    // A vertical gap larger than the incoming word's line box means at least
    // one blank line sat between: a Paragraph boundary. Wrapped lines have
    // leading gaps smaller than their line box, so they stay together.
    // Comparing against the word's own height, not the accumulated paragraph
    // rect, keeps the threshold from growing with a long Paragraph until no
    // real blank-line gap could ever split it.
    let mut paragraphs: Vec<Paragraph> = Vec::new();
    let mut current_lines: Vec<WordBox> = Vec::new();
    let mut current_rect: Option<Rect> = None;
    let mut previous_bottom = 0.0;
    for word in words {
        let gap = word.rect.y - previous_bottom;
        let splits = match current_rect {
            Some(_) => gap > word.rect.h,
            None => false,
        };
        if splits {
            paragraphs.push(Paragraph {
                page,
                words: std::mem::take(&mut current_lines),
                rect: current_rect.take().expect("Some when splitting"),
            });
        }
        let word_bottom = word.rect.y + word.rect.h;
        current_rect = Some(match current_rect {
            Some(rect) => rect.union(word.rect),
            None => word.rect,
        });
        previous_bottom = previous_bottom.max(word_bottom);
        current_lines.push(word);
    }
    if !current_lines.is_empty() {
        paragraphs.push(Paragraph {
            page,
            words: current_lines,
            rect: current_rect.expect("non-empty current_lines"),
        });
    }
    paragraphs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A character box: glyph `ch` at (x, y), 10pt wide, 14pt tall.
    fn ch(ch: char, x: f32, y: f32) -> CharBox {
        CharBox {
            ch,
            rect: Rect {
                x,
                y,
                w: 10.0,
                h: 14.0,
            },
        }
    }

    #[test]
    fn one_line_of_text_forms_one_paragraph() {
        // "Hi there" laid out left to right at x=72, y=100; the space (x=92)
        // is a separator and stays out of every Word Box.
        let chars = vec![
            ch('H', 72.0, 100.0),
            ch('i', 82.0, 100.0),
            ch(' ', 92.0, 100.0),
            ch('t', 97.0, 100.0),
            ch('h', 107.0, 100.0),
            ch('e', 117.0, 100.0),
            ch('r', 127.0, 100.0),
            ch('e', 137.0, 100.0),
        ];

        let paragraphs = group_into_paragraphs(0, &chars);

        assert_eq!(paragraphs.len(), 1);
        let paragraph = &paragraphs[0];
        assert_eq!(paragraph.page, 0);
        assert_eq!(paragraph.text(), "Hi there");
        assert_eq!(paragraph.words.len(), 2);
        assert_eq!(paragraph.words[0].text, "Hi");
        assert_eq!(paragraph.words[1].text, "there");
        // Paragraph bounds run from H's left edge to e's right edge, 14pt tall.
        assert_eq!(
            paragraph.rect,
            Rect {
                x: 72.0,
                y: 100.0,
                w: 75.0,
                h: 14.0
            }
        );
    }

    #[test]
    fn two_close_lines_form_one_paragraph_in_reading_order() {
        // "world" (second line, y=114) deliberately arrives before "Hello"
        // (first line, y=100): Reading Order comes from geometry, not from
        // the order the platform layer reported characters in.
        let mut chars = vec![
            ch('w', 72.0, 114.0),
            ch('o', 82.0, 114.0),
            ch('r', 92.0, 114.0),
            ch('l', 102.0, 114.0),
            ch('d', 112.0, 114.0),
        ];
        chars.extend([
            ch('H', 72.0, 100.0),
            ch('e', 82.0, 100.0),
            ch('l', 92.0, 100.0),
            ch('l', 102.0, 100.0),
            ch('o', 112.0, 100.0),
        ]);

        let paragraphs = group_into_paragraphs(0, &chars);

        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].text(), "Hello world");
        assert_eq!(
            paragraphs[0].rect,
            Rect {
                x: 72.0,
                y: 100.0,
                w: 50.0,
                h: 28.0
            }
        );
    }

    #[test]
    fn a_blank_line_after_a_long_paragraph_still_starts_a_new_one() {
        // Ten wrapped lines in one Paragraph (word chars at 14pt pitch,
        // y=100..226), then a blank line's worth of space, then "Outro". The
        // split threshold must not grow with the accumulated paragraph
        // height (parent of this rule: gap-vs-word-height, see above).
        let mut chars: Vec<CharBox> = Vec::new();
        for line in 0..10 {
            chars.extend([
                ch('x', 72.0, 100.0 + line as f32 * 14.0),
                ch('x', 82.0, 100.0 + line as f32 * 14.0),
                ch('x', 92.0, 100.0 + line as f32 * 14.0),
            ]);
        }
        chars.push(ch('o', 72.0, 256.0));

        let paragraphs = group_into_paragraphs(0, &chars);

        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[1].text(), "o");
    }

    #[test]
    fn same_line_word_with_shifted_baseline_keeps_left_to_right_order() {
        // "n small" where the "small" run sits 3pt higher (superscript-ish).
        // Sorting purely by top-y would put it before the run at y=100;
        // vertical overlap keeps the visual line intact and x orders words.
        let chars = vec![
            ch('n', 72.0, 100.0),
            ch(' ', 82.0, 100.0),
            CharBox {
                ch: 's',
                rect: Rect {
                    x: 92.0,
                    y: 97.0,
                    w: 10.0,
                    h: 14.0,
                },
            },
            ch('m', 102.0, 97.0),
            ch('a', 112.0, 97.0),
            ch('l', 122.0, 97.0),
            ch('l', 132.0, 97.0),
        ];

        let paragraphs = group_into_paragraphs(0, &chars);

        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].text(), "n small");
    }

    #[test]
    fn adjacent_lines_with_slightly_overlapping_boxes_stay_separate_lines() {
        // Two physical lines at 13pt pitch with 14pt-tall boxes spill 1pt
        // into each other; the majority-overlap rule must not glue them, or
        // the words interleave ("Alpha a second line line one right ...").
        let chars = vec![
            ch('b', 72.0, 113.0),
            ch('y', 82.0, 113.0),
            ch('a', 72.0, 100.0),
            ch('x', 82.0, 100.0),
        ];

        let paragraphs = group_into_paragraphs(0, &chars);

        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].text(), "ax by");
    }

    #[test]
    fn a_page_with_no_text_yields_no_paragraphs() {
        // Whitespace only — the Silent Document path must find nothing to
        // ever speak or highlight.
        let chars = vec![ch(' ', 72.0, 100.0), ch(' ', 82.0, 100.0)];

        assert!(group_into_paragraphs(0, &chars).is_empty());
        assert!(group_into_paragraphs(0, &[]).is_empty());
    }

    #[test]
    fn a_blank_line_starts_a_new_paragraph() {
        // "Intro" at y=100, then a blank line's worth of vertical space,
        // then "Outro" at y=156: two Paragraphs, in Reading Order.
        let chars = vec![
            ch('I', 72.0, 100.0),
            ch('n', 82.0, 100.0),
            ch('t', 92.0, 100.0),
            ch('r', 102.0, 100.0),
            ch('o', 112.0, 100.0),
            ch('O', 72.0, 156.0),
            ch('u', 82.0, 156.0),
            ch('t', 92.0, 156.0),
            ch('r', 102.0, 156.0),
            ch('o', 112.0, 156.0),
        ];

        let paragraphs = group_into_paragraphs(0, &chars);

        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text(), "Intro");
        assert_eq!(paragraphs[1].text(), "Outro");
        assert_eq!(paragraphs[1].rect.y, 156.0);
    }
}
