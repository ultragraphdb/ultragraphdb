/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a run on both stores did.
#[derive(Debug, Default)]
pub struct Run {
    replayed: usize,
    fenced: usize,
    recoveries: usize,
    versionless_reads: (usize, usize),
}

impl Run {
    /// A run that replayed `replayed` records, fenced `fenced` entries, reopened fjall `recoveries` times and replayed
    /// `versionless_reads` versionless reads, the second of them the reads that found a value.
    pub(crate) fn new(
        replayed: usize,
        fenced: usize,
        recoveries: usize,
        versionless_reads: (usize, usize),
    ) -> Self {
        debug_assert!(versionless_reads.1 <= versionless_reads.0);
        debug_assert!(versionless_reads.0 <= replayed);
        Self {
            replayed,
            fenced,
            recoveries,
            versionless_reads,
        }
    }

    /// How many records were replayed on fjall.
    #[must_use]
    pub fn replayed(&self) -> usize {
        self.replayed
    }

    /// How many entries prefix drops wrote, fenced on both stores.
    #[must_use]
    pub fn fenced(&self) -> usize {
        self.fenced
    }

    /// How many times fjall was reopened after a crash between steps.
    #[must_use]
    pub fn recoveries(&self) -> usize {
        self.recoveries
    }

    /// How many versionless reads were replayed, and how many of them found a value.
    #[must_use]
    pub fn versionless_reads(&self) -> (usize, usize) {
        self.versionless_reads
    }
}
