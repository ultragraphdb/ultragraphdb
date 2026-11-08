/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod closed_view_body_view;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::closed_partition::ClosedPartition;
use crate::types::command::Command;
use crate::utils::le::bytes_at;

/// The size of a Closed body's partition.
pub(crate) const PARTITION_SIZE: usize = 16;

/// The body of a received Closed frame, read in place; see [`crate::Closed`] for its layout. The partitions ascend
/// from 1 and every epoch is at most [`crate::MAX_EPOCH`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosedView<'a> {
    partitions: &'a [u8],
}

impl<'a> ClosedView<'a> {
    /// The number of partitions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.partitions.len() / PARTITION_SIZE
    }

    /// Whether the body names no partition; a checked body names at least one.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.partitions.is_empty()
    }

    /// Partition `index`, `None` past the last.
    #[must_use]
    pub fn partition(&self, index: usize) -> Option<ClosedPartition> {
        let at = index.checked_mul(PARTITION_SIZE)?;
        bytes_at(self.partitions, at).map(|bytes| ClosedPartition::decode(&bytes))
    }

    /// The partitions, in order.
    pub fn partitions(&self) -> impl Iterator<Item = ClosedPartition> + 'a {
        let view = *self;
        (0..view.len()).filter_map(move |index| view.partition(index))
    }

    /// The body as the request the storage process receives.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::field_value`] (`"epoch"`, the epoch) at the first partition whose epoch is not 0.
    pub fn check_request(&self) -> Result<(), ProtocolError> {
        match self.partitions().find(|partition| partition.epoch() != 0) {
            Some(partition) => Err(ProtocolError::field_value(
                Command::Closed.number(),
                "epoch",
                partition.epoch(),
            )),
            None => Ok(()),
        }
    }
}
