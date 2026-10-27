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

use eframe::egui;

fn main() {
    let start_path = std::env::args().nth(1);
    let app = poopdf::app::PdfApp::new(start_path);
    let app = match app {
        Ok(app) => app,
        Err(e) => {
            eprintln!("poopdf: {e}");
            std::process::exit(1);
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("poopdf")
            .with_inner_size([1100.0, 900.0]),
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "poopdf",
        options,
        Box::new(move |_cc| Ok(Box::new(app) as Box<dyn eframe::App>)),
    ) {
        eprintln!("poopdf: {e}");
        std::process::exit(1);
    }
}
