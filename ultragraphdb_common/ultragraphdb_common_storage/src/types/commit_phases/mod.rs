/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::{LatencyHistogram, PhaseWindow};

use crate::types::commit_mark::CommitMark;
use crate::types::commit_phase::CommitPhase;
use crate::types::log_io_totals::LogIoTotals;

/// The phases of a partition's commits over a window ([`CommitPhase`], in a [`PhaseWindow`]), and its log's I/O over the same
/// window: from the first commit's request taken to the last commit's reply handed out. Due as its window is. Data plane:
/// recording allocates nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitPhases {
    window: PhaseWindow<{ CommitPhase::ALL.len() }>,
    first: Option<CommitMark>,
    last: CommitMark,
}

impl CommitPhases {
    /// A window holding nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts a commit whose request was taken at `taken`, which entered the commit step at `stepped` and whose reply was handed
    /// out at `answered`.
    pub fn record(&mut self, taken: &CommitMark, stepped: &CommitMark, answered: &CommitMark) {
        let since = |earlier: &CommitMark, later: &CommitMark| {
            later.at_us().saturating_sub(earlier.at_us())
        };
        let log = answered.log().since(taken.log());
        let phases = CommitPhase::ALL.map(|phase| match phase {
            CommitPhase::Total => since(taken, answered),
            CommitPhase::BeforeStep => since(taken, stepped),
            CommitPhase::InStep => since(stepped, answered),
            CommitPhase::LogWrite => log.write_us(),
            CommitPhase::LogSync => log.sync_us(),
            CommitPhase::Waited => answered.waited_us().saturating_sub(taken.waited_us()),
        });
        self.window.record(taken.at_us(), answered.at_us(), phases);
        if self.first.is_none_or(|first| taken.at_us() < first.at_us()) {
            self.first = Some(*taken);
        }
        if answered.at_us() >= self.last.at_us() {
            self.last = *answered;
        }
    }

    /// Whether the window is due at `now_us` ([`PhaseWindow::is_due`]).
    #[must_use]
    pub fn is_due(&self, now_us: u64) -> bool {
        self.window.is_due(now_us)
    }

    /// The commits counted.
    #[must_use]
    pub fn commits(&self) -> u64 {
        self.window.count()
    }

    /// The microseconds from the first commit's request taken to the last commit's reply handed out.
    #[must_use]
    pub fn window_us(&self) -> u64 {
        self.window.window_us()
    }

    /// What the log spent in its I/O over the window.
    #[must_use]
    pub fn log(&self) -> LogIoTotals {
        self.first.map_or_else(LogIoTotals::default, |first| {
            self.last.log().since(first.log())
        })
    }

    /// The histogram of `phase`, `None` for none: every phase has one.
    #[must_use]
    pub fn histogram(&self, phase: CommitPhase) -> Option<&LatencyHistogram> {
        self.window.histogram(phase as usize)
    }

    /// The median, the 99th percentile and the mean of `phase`.
    #[must_use]
    pub fn figures(&self, phase: CommitPhase) -> (u64, u64, u64) {
        self.window.figures(phase as usize)
    }

    /// Starts a new window, keeping the histograms' buckets.
    pub fn clear(&mut self) {
        self.window.clear();
        self.first = None;
        self.last = CommitMark::default();
    }
}
