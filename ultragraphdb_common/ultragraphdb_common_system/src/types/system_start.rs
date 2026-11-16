/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;

/// What the owner is configured with.
#[derive(Debug, Clone)]
pub struct SystemStart {
    metadata: MetadataConfig,
    sequencer: SequencerConfig,
    partition_count: u32,
    bootstrap_file: Option<String>,
    tenants: Vec<u64>,
    transaction_ms: u64,
    core: usize,
}

impl SystemStart {
    /// `transaction_ms` unless set otherwise, 10,000 ms: the protocol's default (`PROTOCOL.md` §11.9).
    pub const TRANSACTION_MS: u64 = 10_000;

    /// The core the group's runtime takes unless set otherwise, 1: the frontend's, whose endpoint cores are numbered from 0.
    pub const CORE: usize = 1;

    /// The owner of a cluster of `partition_count` partitions.
    #[must_use]
    pub fn new(metadata: MetadataConfig, sequencer: SequencerConfig, partition_count: u32) -> Self {
        Self {
            metadata,
            sequencer,
            partition_count,
            bootstrap_file: None,
            tenants: Vec::new(),
            transaction_ms: Self::TRANSACTION_MS,
            core: Self::CORE,
        }
    }

    /// The start creating the bootstrap administrator from the secret file at `file`, when the group holds no principal.
    #[must_use]
    pub fn with_bootstrap_file(mut self, file: impl Into<String>) -> Self {
        self.bootstrap_file = Some(file.into());
        self
    }

    /// The start creating `tenants` beside tenant 0: what a test holds, no command creating a tenant in this stage.
    #[must_use]
    pub fn with_tenants(mut self, tenants: Vec<u64>) -> Self {
        self.tenants = tenants;
        self
    }

    /// The start of a cluster whose transactions live at most `transaction_ms`: a mark is held for twice it past its
    /// connection's close, and the group is quiet for twice it after it begins to listen (`PROTOCOL.md` §11.5).
    #[must_use]
    pub fn with_transaction_ms(mut self, transaction_ms: u64) -> Self {
        self.transaction_ms = transaction_ms;
        self
    }

    /// The start of a group whose runtime takes the role's `core`th core, from 0, as `[runtime] pin_cores` binds it: a storage
    /// process's after its partitions' (design D3).
    #[must_use]
    pub fn with_core(mut self, core: usize) -> Self {
        self.core = core;
        self
    }

    /// The core the group's runtime takes, from 0.
    #[must_use]
    pub fn core(&self) -> usize {
        self.core
    }

    /// The life of a read-write transaction, in milliseconds.
    #[must_use]
    pub fn transaction_ms(&self) -> u64 {
        self.transaction_ms
    }

    /// The secret file the bootstrap administrator's password is in.
    #[must_use]
    pub fn bootstrap_file(&self) -> Option<&str> {
        self.bootstrap_file.as_deref()
    }

    /// The tenants created beside tenant 0.
    #[must_use]
    pub fn tenants(&self) -> &[u64] {
        &self.tenants
    }

    /// The metadata group's settings.
    #[must_use]
    pub fn metadata(&self) -> &MetadataConfig {
        &self.metadata
    }

    /// The sequencer's settings.
    #[must_use]
    pub fn sequencer(&self) -> &SequencerConfig {
        &self.sequencer
    }

    /// The partitions a first start forms the placement over.
    #[must_use]
    pub fn partition_count(&self) -> u32 {
        self.partition_count
    }
}
