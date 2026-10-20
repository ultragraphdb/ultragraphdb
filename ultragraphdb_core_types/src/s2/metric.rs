/*
 * Copyright 2015 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/metric.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::cellid::MAX_LEVEL;
use crate::s2::gomath;

/// A measure for cells. It is used to describe the shape and size of cells. They are useful
/// for deciding which cell level to use in order to satisfy a given condition (e.g. that cell
/// vertices must be no further than "x" apart).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Metric {
    // Dim is either 1 or 2, for a 1D or 2D metric respectively.
    dim: i64,
    // Deriv is the scaling factor for the metric.
    deriv: f64,
}

/// MinWidthMetric: Each cell is bounded by four planes passing through its four edges and the
/// center of the sphere; this is the minimum distance between two parallel planes. Go's
/// constant `2 * math.Sqrt2 / 3`.
pub(crate) const MIN_WIDTH_METRIC: Metric = Metric {
    dim: 1,
    deriv: f64::from_bits(0x3fee2b7dddfefa66),
};

/// AvgEdgeMetric: the average edge length of a cell, 1.459213746386106062.
pub(crate) const AVG_EDGE_METRIC: Metric = Metric {
    dim: 1,
    deriv: f64::from_bits(0x3ff758f08369a1a5),
};

/// AvgAreaMetric: the average area of a cell, Go's constant `4 * math.Pi / 6`.
pub(crate) const AVG_AREA_METRIC: Metric = Metric {
    dim: 2,
    deriv: f64::from_bits(0x4000c152382d7366),
};

impl Metric {
    /// Returns the value of the metric at the given level.
    pub(crate) fn value(&self, level: i64) -> f64 {
        gomath::ldexp(self.deriv, -self.dim * level)
    }

    /// Returns the minimum level such that the metric is at most the given value, or
    /// MaxLevel (30) if there is no such level.
    pub(crate) fn min_level(&self, val: f64) -> i64 {
        if val < 0.0 {
            return MAX_LEVEL;
        }

        (-(gomath::ilogb(val / self.deriv) >> (self.dim - 1))).clamp(0, MAX_LEVEL)
    }

    /// Returns the maximum level such that the metric is at least the given value, or zero if
    /// there is no such level.
    pub(crate) fn max_level(&self, val: f64) -> i64 {
        if val <= 0.0 {
            return MAX_LEVEL;
        }

        (gomath::ilogb(self.deriv / val) >> (self.dim - 1)).clamp(0, MAX_LEVEL)
    }
}
