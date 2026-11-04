/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use ultragraphdb_cluster_placement::PartitionId;

use crate::types::partition_message::PartitionMessage;

/// A partition's place in the exchange between the partitions of one process: its inbox and a sender to each partition,
/// its own included, which is how a close reaches it from whoever delivers closes.
#[derive(Debug)]
pub struct PartitionMailbox {
    id: PartitionId,
    inbox: Receiver<PartitionMessage>,
    peers: BTreeMap<PartitionId, Sender<PartitionMessage>>,
}

impl PartitionMailbox {
    /// One mailbox per partition of `ids`, in order, each with a sender to all.
    pub fn mesh(ids: &[PartitionId]) -> Vec<Self> {
        let (senders, inboxes): (Vec<_>, Vec<_>) = ids.iter().map(|_| channel()).unzip();
        let peers: BTreeMap<PartitionId, Sender<PartitionMessage>> =
            ids.iter().copied().zip(senders).collect();
        ids.iter()
            .copied()
            .zip(inboxes)
            .map(|(id, inbox)| Self {
                id,
                inbox,
                peers: peers.clone(),
            })
            .collect()
    }

    /// The partition the mailbox belongs to.
    pub fn id(&self) -> PartitionId {
        self.id
    }

    /// A sender to partition `to`, `None` for one outside the mesh.
    pub fn sender(&self, to: PartitionId) -> Option<Sender<PartitionMessage>> {
        self.peers.get(&to).cloned()
    }

    /// The next message waiting, `None` when none is.
    pub(crate) fn next(&self) -> Option<PartitionMessage> {
        self.inbox.try_recv().ok()
    }

    /// Sends `message` to `to`. A partition gone drops it: the asker sends again, and a registration is idempotent.
    pub(crate) fn send(&self, to: PartitionId, message: PartitionMessage) {
        if let Some(sender) = self.peers.get(&to) {
            // A receiver gone is a partition that stopped; see above.
            drop(sender.send(message));
        }
    }
}
