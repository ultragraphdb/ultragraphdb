/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;

use crate::types::corruption::Corruption;
use crate::types::drop_stage::DropStage;
use crate::types::fault::Fault;
use crate::types::fault_config::FaultConfig;
use crate::types::sim_rng::SimRng;

/// What happens to a batch the store is asked to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchOutcome {
    Written,
    Failed,
    Crashed,
}

/// The faults a simulated store injects: those forced on it first, then those its stream draws.
#[derive(Debug, Clone)]
pub struct FaultInjector {
    pub rng: SimRng,
    config: FaultConfig,
    forced: VecDeque<Fault>,
}

impl FaultInjector {
    pub fn new(rng: SimRng, config: FaultConfig) -> Self {
        Self {
            rng,
            config,
            forced: VecDeque::new(),
        }
    }

    pub fn force(&mut self, fault: Fault) {
        self.forced.push_back(fault);
    }

    fn take_forced(&mut self, matches: impl Fn(&Fault) -> bool) -> Option<Fault> {
        let at = self.forced.iter().position(matches)?;
        self.forced.remove(at)
    }

    pub fn batch(&mut self) -> BatchOutcome {
        match self.take_forced(|f| matches!(f, Fault::FailNextBatch | Fault::CrashDuringNextBatch))
        {
            Some(Fault::FailNextBatch) => BatchOutcome::Failed,
            Some(_) => BatchOutcome::Crashed,
            None if self.rng.chance(self.config.batch_failure()) => BatchOutcome::Failed,
            None if self.rng.chance(self.config.crash_during_batch()) => BatchOutcome::Crashed,
            None => BatchOutcome::Written,
        }
    }

    /// Where the next drop stops, if it does: the stage, and true for a power loss or false for a crash.
    pub fn drop_interruption(&mut self) -> Option<(DropStage, bool)> {
        let forced = self.take_forced(|f| {
            matches!(
                f,
                Fault::CrashDuringNextDrop(_) | Fault::PowerLossDuringNextDrop(_)
            )
        });
        match forced {
            Some(Fault::PowerLossDuringNextDrop(stage)) => Some((stage, true)),
            Some(Fault::CrashDuringNextDrop(stage)) => Some((stage, false)),
            _ if self.rng.chance(self.config.power_loss_during_drop()) => {
                Some((self.stage(), true))
            }
            _ if self.rng.chance(self.config.crash_during_drop()) => Some((self.stage(), false)),
            _ => None,
        }
    }

    fn stage(&mut self) -> DropStage {
        DropStage::ALL[self.rng.index(DropStage::ALL.len())]
    }

    pub fn read_fails(&mut self) -> bool {
        self.take_forced(|f| *f == Fault::FailNextRead).is_some()
            || self.rng.chance(self.config.read_failure())
    }

    /// After how many of `len` items the scan fails, if it does.
    pub fn scan_fails_after(&mut self, len: usize) -> Option<usize> {
        match self.take_forced(|f| matches!(f, Fault::FailNextScanAfter(_))) {
            Some(Fault::FailNextScanAfter(n)) => Some(n.min(len)),
            _ if self.rng.chance(self.config.scan_failure()) => Some(self.rng.index(len + 1)),
            _ => None,
        }
    }

    pub fn corruption(&mut self) -> Option<Corruption> {
        self.rng.chance(self.config.corruption()).then(|| {
            if self.rng.chance(500) {
                Corruption::Truncate
            } else {
                Corruption::FlipValueByte
            }
        })
    }

    /// The delay of a slow operation, if this one is.
    pub fn slow(&mut self) -> Option<u64> {
        self.rng
            .chance(self.config.slow())
            .then(|| self.rng.between(1, self.config.slow_max_nanos()))
    }
}
