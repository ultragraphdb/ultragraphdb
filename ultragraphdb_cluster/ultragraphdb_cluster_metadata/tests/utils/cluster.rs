/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The metadata group beside the real sequencer, as the frontend hosts them: both over the write-ahead log on one
//! simulated storage, the sequencer's close announcing the versions the group holds pending and carrying its pending
//! evictions, and the group learning each durable close. The BDD target's alone, which includes it by path.

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_cluster_sequencer::{
    CatalogVersion, ClockReading, Close, Sequencer, SequencerConfig,
};
use ultragraphdb_core_ids::Namespace;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_log::Wal;

use crate::harness::{Group, Log, block_on, fresh, sync, wal_config};

/// The sequencer's log directory, beside the group's.
pub const SEQUENCER_DIR: &str = "/seq";

/// The partition number the host subscribes as, to learn every close.
const HOST: u32 = 1_000;

/// The metadata group and the sequencer of one frontend.
pub struct Cluster {
    pub group: Group,
    pub sequencer: Sequencer<Log>,
}

impl Cluster {
    /// A fresh group and a fresh sequencer closing every millisecond.
    pub fn new() -> TestResult<Self> {
        let (storage, group) = fresh()?;
        let (log, _) = block_on(Wal::open(storage.clone(), SEQUENCER_DIR, wal_config()))??;
        let config = SequencerConfig::new(1, 1)?;
        let mut sequencer = block_on(Sequencer::recover(log, config, ClockReading::new(0, 0)))??;
        sequencer.subscribe(PartitionId::new(HOST)?, ultragraphdb_core_ids::Epoch::ZERO)?;
        Ok(Self { group, sequencer })
    }

    /// Closes the open epoch at its deadline, announcing the versions the group holds pending and carrying its pending
    /// evictions; once the close is durable, the group learns it. Returns the close.
    pub fn close(&mut self) -> TestResult<Close> {
        let now = self.sequencer.deadline();
        let versions: Vec<CatalogVersion> = self
            .group
            .state()
            .pending_versions()
            .into_iter()
            .map(|(namespace, version)| CatalogVersion::new(namespace, version))
            .collect();
        let evictions = self.group.state().pending_evictions();
        let _closed = self
            .sequencer
            .tick(ClockReading::new(now, now), &versions, &evictions)?
            .context("a close due at its deadline")?;
        let _durable = block_on(self.sequencer.sync())??;
        let close = self
            .sequencer
            .take_delivery(PartitionId::new(HOST)?)?
            .context("the durable close")?;
        let announced: Vec<(Namespace, u64)> = close
            .catalog()
            .iter()
            .map(|version| (version.namespace(), version.version()))
            .collect();
        let _learned = self
            .group
            .learn_close(close.epoch(), &announced, close.evictions())?;
        sync(&mut self.group)?;
        Ok(close)
    }

    /// Closes epochs until `through` is the latest close; returns every close made.
    pub fn close_through(&mut self, through: u64) -> TestResult<Vec<Close>> {
        let mut closes = Vec::new();
        for _ in 0..through {
            if self.sequencer.last_closed().get() >= through {
                break;
            }
            closes.push(self.close()?);
        }
        Ok(closes)
    }
}
