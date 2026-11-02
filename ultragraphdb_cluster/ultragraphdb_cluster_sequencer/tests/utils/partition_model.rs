/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A model of a partition's side of the epoch protocol, which the scenarios of "Partitions learn each closed epoch and
//! stamp the open one" drive with the sequencer's deliveries: it stamps one above the latest close delivered to it,
//! never advancing on its own, holds the catalog versions each close announces, and has closed an epoch once its close
//! is delivered and every round stamped at or below it is applied. The partition's own code takes these rules in
//! group 4 (the epoch stamp) and group 7 (the partition), where its scenarios run against it.

use ultragraphdb_cluster_sequencer::{CatalogVersion, Close};
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// One partition's view of the closes delivered to it.
#[derive(Debug, Default)]
pub struct PartitionModel {
    /// The latest close delivered.
    delivered: u64,
    /// The epochs of the rounds stamped and not yet applied.
    unapplied: Vec<u64>,
    /// Each catalog version held, with the epoch it takes effect at.
    catalog: Vec<(CatalogVersion, u64)>,
}

impl PartitionModel {
    /// Takes the delivery of `close`, which must be the close after the latest delivered.
    pub fn receive(&mut self, close: &Close) -> TestResult {
        if close.epoch().get() != self.delivered + 1 {
            return Err(TestError::new(format!(
                "close {} delivered after close {}",
                close.epoch().get(),
                self.delivered
            )));
        }
        self.delivered = close.epoch().get();
        let effective = self.delivered + 1;
        self.catalog
            .extend(close.catalog().iter().map(|version| (*version, effective)));
        Ok(())
    }

    /// The latest close delivered.
    pub fn delivered(&self) -> u64 {
        self.delivered
    }

    /// The epoch a commit is stamped with now: one above the latest close delivered.
    pub fn stamp(&self) -> u64 {
        self.delivered + 1
    }

    /// Commits a round at the open epoch, not yet applied; returns its epoch.
    pub fn commit_round(&mut self) -> u64 {
        let epoch = self.stamp();
        self.unapplied.push(epoch);
        epoch
    }

    /// Applies every round committed.
    pub fn apply_rounds(&mut self) {
        self.unapplied.clear();
    }

    /// The latest epoch the partition has closed: delivered, with no round at or below it unapplied.
    pub fn closed(&self) -> u64 {
        let lowest_unapplied = self.unapplied.iter().min().copied().unwrap_or(u64::MAX);
        self.delivered.min(lowest_unapplied.saturating_sub(1))
    }

    /// Whether `version` is held, taking effect at or before `epoch`.
    pub fn holds(&self, version: CatalogVersion, epoch: u64) -> bool {
        self.catalog
            .iter()
            .any(|(held, effective)| *held == version && *effective <= epoch)
    }
}
