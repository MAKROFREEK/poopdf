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

//! poopdf: a local-first, keyboard-driven PDF viewer that reads documents
//! aloud. GPL-3.0-or-later. Architecture decisions live in docs/adr/, the
//! domain language in GLOSSARY.md.

pub mod app;
pub mod document;
pub mod page_cache;
pub mod pdfium_lib;
pub mod view;
pub mod zoom;
