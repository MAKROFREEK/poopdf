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

//! Zoom state: stepping, resetting, and clamping of the view zoom factor.
//!
//! Zoom is the render scale of pages in logical pixels per PDF point.
//! One step multiplies (or divides) by [`STEP`]; the factor is clamped to
//! [`MIN_FACTOR`]..=[`MAX_FACTOR`] so bitmaps never grow past texture limits.

/// Zoom is multiplicative per step.
pub const STEP: f32 = 1.25;
/// Minimum zoom factor: 25%.
pub const MIN_FACTOR: f32 = 0.25;
/// Maximum zoom factor: a 612x792pt page renders within the 8192px texture limit.
pub const MAX_FACTOR: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zoom(f32);

impl Default for Zoom {
    fn default() -> Self {
        Self(1.0)
    }
}

impl Zoom {
    pub fn new(factor: f32) -> Self {
        Self(factor.clamp(MIN_FACTOR, MAX_FACTOR))
    }

    /// The current zoom factor: logical pixels per PDF point.
    pub fn factor(self) -> f32 {
        self.0
    }

    /// One zoom step up (`+`, or a positive Ctrl+wheel delta).
    pub fn step_in(self) -> Self {
        Self::new(self.0 * STEP)
    }

    /// One zoom step down (`−`, or a negative Ctrl+wheel delta).
    pub fn step_out(self) -> Self {
        Self::new(self.0 / STEP)
    }

    /// A positive wheel delta zooms in, negative zooms out.
    pub fn wheel(self, delta: f32) -> Self {
        if delta > 0.0 {
            self.step_in()
        } else {
            self.step_out()
        }
    }

    /// Back to the default factor (`0`).
    pub fn reset(self) -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_reset_factor() {
        assert_eq!(Zoom::default().factor(), 1.0);
    }

    #[test]
    fn step_in_multiplies() {
        let z = Zoom::default().step_in();
        assert!((z.factor() - 1.25).abs() < 1e-6);
    }

    #[test]
    fn step_out_divides() {
        let z = Zoom::default().step_out();
        assert!((z.factor() - 1.0 / 1.25).abs() < 1e-6);
    }

    #[test]
    fn step_in_clamps_at_max() {
        let z = Zoom::new(MAX_FACTOR).step_in();
        assert_eq!(z.factor(), MAX_FACTOR);
    }

    #[test]
    fn step_out_clamps_at_min() {
        let z = Zoom::new(MIN_FACTOR).step_out();
        assert_eq!(z.factor(), MIN_FACTOR);
    }

    #[test]
    fn set_factor_clamps() {
        assert_eq!(Zoom::new(100.0).factor(), MAX_FACTOR);
        assert_eq!(Zoom::new(0.01).factor(), MIN_FACTOR);
        assert_eq!(Zoom::new(2.0).factor(), 2.0);
    }

    #[test]
    fn wheel_direction_scales_the_factor() {
        let z = Zoom::default().wheel(120.0);
        assert!((z.factor() - 1.25).abs() < 1e-6, "positive delta zooms in");
        let z = Zoom::default().wheel(-120.0);
        assert!(
            (z.factor() - 1.0 / 1.25).abs() < 1e-6,
            "negative delta zooms out"
        );
    }

    #[test]
    fn reset_returns_to_default() {
        let z = Zoom::new(3.0).reset();
        assert_eq!(z.factor(), 1.0);
    }
}
