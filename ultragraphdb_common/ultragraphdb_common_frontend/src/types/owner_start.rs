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
}

impl OwnerStart {
    /// The owner of a cluster of `partition_count` partitions.
    #[must_use]
    pub fn new(metadata: MetadataConfig, sequencer: SequencerConfig, partition_count: u32) -> Self {
        Self {
            metadata,
            sequencer,
            partition_count,
        }
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
