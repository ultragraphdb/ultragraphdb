/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The nearest-rank percentiles, the maximum and the mean of a run's samples.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Summary {
    count: usize,
    p50: u64,
    p90: u64,
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
        // A rank is clamped to `1..=n`, so every read is of a sample; `values` is not empty. `q` lies in 0 to 1, so the
        // product lies in 0 to `n`, which the saturating cast into a `usize` keeps whole.
        let rank = |q: f64| {
            // The rank is one or above for a quantile above zero; the float saturates into an i64 first.
            let rank = usize::try_from((q * n as f64).ceil() as i64).unwrap_or(1);
            let i = rank.clamp(1, n) - 1;
            values.get(i).copied().unwrap_or_default()
        };
        let summary = Self {
            count: n,
            p50: rank(0.5),
            p90: rank(0.9),
            p99: rank(0.99),
            max: values.last().copied().unwrap_or_default(),
            mean: values.iter().map(|&v| v as f64).sum::<f64>() / n as f64,
        };
        debug_assert!(summary.p50 <= summary.p90);
        debug_assert!(summary.p90 <= summary.p99);
        debug_assert!(summary.p99 <= summary.max);
        summary
    }

    /// The number of samples.
    pub fn count(&self) -> usize {
        self.count
    }

    /// The nearest-rank median.
    pub fn p50(&self) -> u64 {
        self.p50
    }

    /// The nearest-rank 90th percentile.
    pub fn p90(&self) -> u64 {
        self.p90
    }

    /// The nearest-rank 99th percentile.
    pub fn p99(&self) -> u64 {
        self.p99
    }

    /// The largest sample.
    pub fn max(&self) -> u64 {
        self.max
    }

    /// The mean of the samples.
    pub fn mean(&self) -> f64 {
        self.mean
    }
}
