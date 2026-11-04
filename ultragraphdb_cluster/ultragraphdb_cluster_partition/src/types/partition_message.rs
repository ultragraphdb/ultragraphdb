/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_cluster_sequencer::Close;

use crate::errors::partition_error::PartitionError;
use crate::types::answer::Answer;
use crate::types::reference::Reference;

/// What the partitions of one process send each other (term-dictionary, "A reference to another partition's entity
/// registers at its home"): storage processes carry the same exchange as a role command.
#[derive(Debug)]
pub enum PartitionMessage {
    /// Registrations of one write request from partition `from`, batched for this home, each with its index in the
    /// request's references.
    Register {
        /// The partition asking.
        from: PartitionId,
        /// The asking write's ticket at `from`.
        write: u64,
        /// The references, each with its index.
        batch: Vec<(usize, Reference)>,
    },
    /// The answer to registration `index` of the write `write`, once durable at its home.
    Answered {
        /// The asking write's ticket.
        write: u64,
        /// The registration's index in its references.
        index: usize,
        /// The home's answer, or why it refused.
        answer: Result<Answer, PartitionError>,
    },
    /// A close of the sequencer, delivered to the partition.
    Close(Close),
}
