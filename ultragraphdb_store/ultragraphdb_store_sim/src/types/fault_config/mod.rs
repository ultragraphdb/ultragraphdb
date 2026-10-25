/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How often a run's fault stream injects each fault, in chances per thousand: per batch for a failed batch and a
/// crash while one is written, per step for a crash or a power loss between steps, per drop for a crash or a power
/// loss during one, per rebuild (each rebuild record written) for a crash or a power loss at one of its stages, per
/// read for a failed read, a failed scan and a corrupted value, per operation for a slow one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FaultConfig {
    batch_failure: u32,
    crash_during_batch: u32,
    crash_between_steps: u32,
    power_loss_between_steps: u32,
    crash_during_drop: u32,
    power_loss_during_drop: u32,
    crash_during_rebuild: u32,
    power_loss_during_rebuild: u32,
    read_failure: u32,
    scan_failure: u32,
    corruption: u32,
    slow: u32,
    slow_max_nanos: u64,
}

impl FaultConfig {
    /// No faults at all.
    pub fn none() -> Self {
        Self::default()
    }

    /// Every fault class, each rare enough that runs of a few hundred steps see most of them.
    pub fn mixed() -> Self {
        Self {
            batch_failure: 8,
            crash_during_batch: 8,
            crash_between_steps: 5,
            power_loss_between_steps: 5,
            crash_during_drop: 150,
            power_loss_during_drop: 150,
            crash_during_rebuild: 150,
            power_loss_during_rebuild: 150,
            read_failure: 10,
            scan_failure: 20,
            corruption: 3,
            slow: 50,
            slow_max_nanos: 50_000_000,
        }
    }

    pub fn with_batch_failure(mut self, per_mille: u32) -> Self {
        self.batch_failure = per_mille;
        self
    }

    pub fn with_crash_during_batch(mut self, per_mille: u32) -> Self {
        self.crash_during_batch = per_mille;
        self
    }

    pub fn with_crash_between_steps(mut self, per_mille: u32) -> Self {
        self.crash_between_steps = per_mille;
        self
    }

    pub fn with_power_loss_between_steps(mut self, per_mille: u32) -> Self {
        self.power_loss_between_steps = per_mille;
        self
    }

    pub fn with_crash_during_drop(mut self, per_mille: u32) -> Self {
        self.crash_during_drop = per_mille;
        self
    }

    pub fn with_power_loss_during_drop(mut self, per_mille: u32) -> Self {
        self.power_loss_during_drop = per_mille;
        self
    }

    pub fn with_crash_during_rebuild(mut self, per_mille: u32) -> Self {
        self.crash_during_rebuild = per_mille;
        self
    }

    pub fn with_power_loss_during_rebuild(mut self, per_mille: u32) -> Self {
        self.power_loss_during_rebuild = per_mille;
        self
    }

    pub fn with_read_failure(mut self, per_mille: u32) -> Self {
        self.read_failure = per_mille;
        self
    }

    pub fn with_scan_failure(mut self, per_mille: u32) -> Self {
        self.scan_failure = per_mille;
        self
    }

    pub fn with_corruption(mut self, per_mille: u32) -> Self {
        self.corruption = per_mille;
        self
    }

    /// Slow operations, each delayed by up to `max_nanos` of simulated time.
    pub fn with_slow(mut self, per_mille: u32, max_nanos: u64) -> Self {
        self.slow = per_mille;
        self.slow_max_nanos = max_nanos;
        self
    }

    pub fn batch_failure(&self) -> u32 {
        self.batch_failure
    }

    pub fn crash_during_batch(&self) -> u32 {
        self.crash_during_batch
    }

    pub fn crash_between_steps(&self) -> u32 {
        self.crash_between_steps
    }

    pub fn power_loss_between_steps(&self) -> u32 {
        self.power_loss_between_steps
    }

    pub fn crash_during_drop(&self) -> u32 {
        self.crash_during_drop
    }

    pub fn power_loss_during_drop(&self) -> u32 {
        self.power_loss_during_drop
    }

    pub fn crash_during_rebuild(&self) -> u32 {
        self.crash_during_rebuild
    }

    pub fn power_loss_during_rebuild(&self) -> u32 {
        self.power_loss_during_rebuild
    }

    pub fn read_failure(&self) -> u32 {
        self.read_failure
    }

    pub fn scan_failure(&self) -> u32 {
        self.scan_failure
    }

    pub fn corruption(&self) -> u32 {
        self.corruption
    }

    pub fn slow(&self) -> u32 {
        self.slow
    }

    pub fn slow_max_nanos(&self) -> u64 {
        self.slow_max_nanos
    }
}
