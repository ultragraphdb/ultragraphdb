/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/earth.go (Length). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt;

/// Dgraph's `Length`: a distance on Earth in metres.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Length(f64);

impl Length {
    pub fn new(metres: f64) -> Self {
        Self(metres)
    }

    pub fn metres(self) -> f64 {
        self.0
    }
}

impl fmt::Display for Length {
    /// Dgraph's `String`: km above 1000 m, cm below 1 m, three decimals.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let l = self.0;
        let (v, unit) = if l > 1000.0 {
            (l / 1000.0, "km")
        } else if l < 1.0 {
            (l * 100.0, "cm")
        } else {
            (l, "m")
        };
        // Go's %.3f writes an infinity as +Inf or -Inf, where Rust writes inf or -inf.
        if v.is_infinite() {
            let sign = if v > 0.0 { '+' } else { '-' };
            return write!(f, "{sign}Inf {unit}");
        }
        write!(f, "{v:.3} {unit}")
    }
}
