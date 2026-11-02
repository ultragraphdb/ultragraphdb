/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterError;

/// A multi-partition transaction as the sequencer's log holds it: the epoch it was placed in, the largest reply epoch
/// among its registrations, at or below that epoch, the partitions it names, each once, and its body, which the
/// sequencer does not read.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SequencedTransaction {
    epoch: Epoch,
    reply_epoch: Epoch,
    partitions: Vec<PartitionId>,
    body: Vec<u8>,
}

impl SequencedTransaction {
    /// The most partitions one transaction names, the placement map's limit, 1,024.
    pub const PARTITIONS_MAX: usize = 1_024;

    /// The most bytes of a transaction's body, 65,536.
    pub const BODY_BYTES_MAX: usize = 65_536;

    /// The transaction placed in `epoch`, naming the reply epoch `reply_epoch`, the partitions `partitions` and the
    /// body `body`.
    ///
    /// # Errors
    ///
    /// In this order: [`ClusterError::no_partitions`] when `partitions` is empty; [`ClusterError::too_many`] when it
    /// names more than [`SequencedTransaction::PARTITIONS_MAX`] partitions or the body holds more than
    /// [`SequencedTransaction::BODY_BYTES_MAX`] bytes; [`ClusterError::duplicate_partition`] naming the first partition
    /// named twice; [`ClusterError::epoch_ahead`] when `reply_epoch` is above `epoch`.
    pub fn new(
        epoch: Epoch,
        reply_epoch: Epoch,
        partitions: Vec<PartitionId>,
        body: Vec<u8>,
    ) -> Result<Self, ClusterError> {
        if partitions.is_empty() {
            return Err(ClusterError::no_partitions());
        }
        if partitions.len() > Self::PARTITIONS_MAX {
            return Err(ClusterError::too_many(
                "partitions of a transaction",
                partitions.len(),
                Self::PARTITIONS_MAX,
            ));
        }
        if body.len() > Self::BODY_BYTES_MAX {
            return Err(ClusterError::too_many(
                "bytes of a transaction's body",
                body.len(),
                Self::BODY_BYTES_MAX,
            ));
        }
        let mut seen = BTreeSet::new();
        if let Some(repeated) = partitions.iter().find(|p| !seen.insert(**p)) {
            return Err(ClusterError::duplicate_partition(repeated.get()));
        }
        if reply_epoch > epoch {
            return Err(ClusterError::epoch_ahead(reply_epoch.get(), epoch.get()));
        }
        Ok(Self {
            epoch,
            reply_epoch,
            partitions,
            body,
        })
    }

    /// The epoch the transaction was placed in.
    #[must_use]
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    /// The largest reply epoch among its registrations.
    #[must_use]
    pub fn reply_epoch(&self) -> Epoch {
        self.reply_epoch
    }

    /// The partitions it names, in the order given.
    #[must_use]
    pub fn partitions(&self) -> &[PartitionId] {
        &self.partitions
    }

    /// Its body.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}
