/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::latency_histogram::LatencyHistogram;

/// How long a window goes without a request before what it counted is due: a second.
pub const PHASES_QUIET_US: u64 = 1_000_000;

/// How long a window of requests runs at most before what it counted is due: a minute.
pub const PHASES_WINDOW_US: u64 = 60_000_000;

/// The `N` phases of a role's requests over a window, each a [`LatencyHistogram`], for the role to log once the window is due: a
/// second after its last request ended, or a minute after its first began, so a run of requests followed by a pause is counted
/// as one and a log line comes at most once a second whatever the load. Times are microseconds on the caller's clock. Data
/// plane: recording allocates nothing; the histograms, 3.7 KiB each, are allocated once by [`PhaseWindow::new`] and kept by
/// [`PhaseWindow::clear`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseWindow<const N: usize> {
    histograms: [LatencyHistogram; N],
    first_us: Option<u64>,
    last_us: u64,
}

impl<const N: usize> Default for PhaseWindow<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> PhaseWindow<N> {
    /// A window holding nothing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            histograms: std::array::from_fn(|_| LatencyHistogram::new()),
            first_us: None,
            last_us: 0,
        }
    }

    /// Counts a request that began at `began_us` and ended at `ended_us`, its phases `phases` in the window's order.
    pub fn record(&mut self, began_us: u64, ended_us: u64, phases: [u64; N]) {
        self.histograms
            .iter_mut()
            .zip(phases)
            .for_each(|(histogram, us)| histogram.record(us));
        if self.first_us.is_none_or(|first| began_us < first) {
            self.first_us = Some(began_us);
        }
        self.last_us = self.last_us.max(ended_us);
    }

    /// Whether the window is due at `now_us`: it holds a request, and either none ended for [`PHASES_QUIET_US`] or its first
    /// began [`PHASES_WINDOW_US`] ago.
    #[must_use]
    pub fn is_due(&self, now_us: u64) -> bool {
        self.first_us.is_some_and(|first| {
            now_us.saturating_sub(self.last_us) >= PHASES_QUIET_US
                || now_us.saturating_sub(first) >= PHASES_WINDOW_US
        })
    }

    /// The requests counted.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.histograms.first().map_or(0, LatencyHistogram::count)
    }

    /// When its first request began, `None` while it holds none.
    #[must_use]
    pub fn first_us(&self) -> Option<u64> {
        self.first_us
    }

    /// The microseconds from its first request's beginning to its last request's end.
    #[must_use]
    pub fn window_us(&self) -> u64 {
        self.first_us
            .map_or(0, |first| self.last_us.saturating_sub(first))
    }

    /// The histogram of phase `index`, `None` past the last.
    #[must_use]
    pub fn histogram(&self, index: usize) -> Option<&LatencyHistogram> {
        self.histograms.get(index)
    }

    /// The median, the 99th percentile and the mean of phase `index`, each 0 where it holds nothing or past the last phase.
    #[must_use]
    pub fn figures(&self, index: usize) -> (u64, u64, u64) {
        self.histogram(index).map_or((0, 0, 0), |histogram| {
            (
                histogram.quantile(500),
                histogram.quantile(990),
                histogram.mean(),
            )
        })
    }

    /// Starts a new window, keeping the histograms' buckets.
    pub fn clear(&mut self) {
        self.histograms.iter_mut().for_each(LatencyHistogram::clear);
        self.first_us = None;
        self.last_us = 0;
    }
}
