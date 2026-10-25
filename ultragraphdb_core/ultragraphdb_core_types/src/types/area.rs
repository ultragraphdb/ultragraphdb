/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/earth.go (Area). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt;

const KM2: f64 = 1000.0 * 1000.0;
const CM2: f64 = 100.0 * 100.0;

/// Dgraph's `Area`: an area on Earth in square metres.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Area(f64);

impl Area {
    pub fn new(square_metres: f64) -> Self {
        Self(square_metres)
    }

    pub fn square_metres(self) -> f64 {
        self.0
    }
}

impl fmt::Display for Area {
    /// Dgraph's `String`: km^2 above 1 km^2, cm^2 below 1 m^2, three decimals.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let a = self.0;
        let (v, unit) = if a > KM2 {
            (a / KM2, "km^2")
        } else if a < 1.0 {
            (a * CM2, "cm^2")
        } else {
            (a, "m^2")
        };
        // Go's %.3f writes an infinity as +Inf or -Inf, where Rust writes inf or -inf.
        if v.is_infinite() {
            let sign = if v > 0.0 { '+' } else { '-' };
            return write!(f, "{sign}Inf {unit}");
        }
        write!(f, "{v:.3} {unit}")
    }
}
