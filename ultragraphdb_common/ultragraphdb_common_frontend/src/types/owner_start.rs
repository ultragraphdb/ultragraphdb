/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;

/// What the owner is configured with.
#[derive(Debug, Clone)]
pub struct OwnerStart {
    metadata: MetadataConfig,
    sequencer: SequencerConfig,
    partition_count: u32,
    bootstrap_file: Option<String>,
    tenants: Vec<u64>,
}

impl OwnerStart {
    /// The owner of a cluster of `partition_count` partitions.
    #[must_use]
    pub fn new(metadata: MetadataConfig, sequencer: SequencerConfig, partition_count: u32) -> Self {
        Self {
            metadata,
            sequencer,
            partition_count,
            bootstrap_file: None,
            tenants: Vec::new(),
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
