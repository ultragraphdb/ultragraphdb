/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Models of a partition, as the group's scenarios see one: its epochs and catalog loading, and its sequence
//! allocator. The BDD target's alone, which includes it by path.

use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_traits_blocks::{BlockSource, SequenceBlock};
use ultragraphdb_traits_catalog::CatalogSource;

/// A partition's view of the epochs: it stamps the epoch after the last close delivered to it, and validates a round
/// against the catalog version the group answers at the round's epoch.
#[derive(Debug, Default)]
pub struct EpochModel {
    delivered: u64,
}

impl EpochModel {
    /// A partition whose latest delivered close is `delivered`.
    pub fn at(delivered: u64) -> Self {
        Self { delivered }
    }

    /// The close `close` delivered.
    pub fn deliver(&mut self, close: &Close) {
        self.delivered = close.epoch().get();
    }

    /// The epoch the partition's next commit carries: the open one, after the last close delivered.
    pub fn stamp(&self) -> u64 {
        self.delivered + 1
    }

    /// The number and contents of the version of `tenant` a round of `epoch` is validated against, 0 and none for
    /// the initial catalog.
    pub fn validate<S: CatalogSource>(
        &self,
        source: &S,
        tenant: u64,
        epoch: u64,
    ) -> Result<(u64, Vec<u8>), S::Error> {
        Ok(source
            .version_at(tenant, epoch)?
            .map_or((0, Vec::new()), |held| {
                (held.version(), held.contents().to_vec())
            }))
    }
}

/// A partition's sequence allocator, as the store's: per type, the next sequence of its current block, and the block
/// after it leased as soon as the current one is begun; a sequence is allocated only from a block held.
#[derive(Debug)]
pub struct AllocatorModel<S> {
    source: S,
    current: Option<(u64, u64)>,
    ahead: Option<SequenceBlock>,
}

impl<S: BlockSource> AllocatorModel<S> {
    /// An allocator leasing from `source`, holding no block.
    pub fn new(source: S) -> Self {
        Self {
            source,
            current: None,
            ahead: None,
        }
    }

    /// The next sequence of `prefix`, or the refusal naming the type when no block is held.
    pub fn next(&mut self, prefix: TypePrefix) -> Result<u64, String> {
        if self.current.is_none_or(|(next, end)| next >= end) {
            self.current = self
                .ahead
                .take()
                .or_else(|| self.source.lease(prefix))
                .map(|block| (block.start().get(), block.end()));
        }
        if self.ahead.is_none() {
            self.ahead = self.source.lease(prefix);
        }
        match self.current {
            Some((next, end)) if next < end => {
                self.current = Some((next + 1, end));
                Ok(next)
            }
            Some(_) | None => Err(format!(
                "no sequence of type {} (Resource is 1): no block is held",
                prefix.get()
            )),
        }
    }
}
