/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The nearest-rank percentiles, the maximum and the mean of a run's samples.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Summary {
    count: usize,
    p50: u64,
    p99: u64,
    max: u64,
    mean: f64,
}

impl Summary {
    /// The summary of `values`, which it sorts.
    pub fn of(values: &mut [u64]) -> Self {
        if values.is_empty() {
            return Self::default();
        }
        values.sort_unstable();
        let n = values.len();
        let rank = |q: f64| values[((q * n as f64).ceil() as usize).clamp(1, n) - 1];
        Self {
            count: n,
            p50: rank(0.5),
            p99: rank(0.99),
            max: values[n - 1],
            mean: values.iter().map(|&v| v as f64).sum::<f64>() / n as f64,
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn p50(&self) -> u64 {
        self.p50
    }

    pub fn p99(&self) -> u64 {
        self.p99
    }

    pub fn max(&self) -> u64 {
        self.max
    }

    pub fn mean(&self) -> f64 {
        self.mean
    }
}
