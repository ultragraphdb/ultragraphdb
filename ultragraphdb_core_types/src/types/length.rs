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
        if l > 1000.0 {
            write!(f, "{:.3} km", l / 1000.0)
        } else if l < 1.0 {
            write!(f, "{:.3} cm", l * 100.0)
        } else {
            write!(f, "{l:.3} m")
        }
    }
}
