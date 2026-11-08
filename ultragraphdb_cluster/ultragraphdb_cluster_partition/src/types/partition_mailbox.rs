/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use ultragraphdb_cluster_placement::PartitionId;

use crate::errors::partition_error::PartitionError;
use crate::types::answer::Answer;
use crate::types::partition_message::{PartitionMessage, RemoteMessage};

/// A partition's place in the exchange between the partitions of one process: its inbox and a sender to each partition,
/// its own included, which is how a close reaches it from whoever delivers closes.
#[derive(Debug)]
pub struct PartitionMailbox {
    id: PartitionId,
    inbox: Receiver<PartitionMessage>,
    peers: BTreeMap<PartitionId, Sender<PartitionMessage>>,
    remote: Option<Sender<RemoteMessage>>,
}

impl PartitionMailbox {
    /// One mailbox per partition of `ids`, in order, each with a sender to all. An inbox has no capacity of its own:
    /// what bounds it is the writes parked at its peers, each sending one batch a second until its read-write limit
    /// ends it, the answers to the registrations the partition sent, and the sequencer's closes.
    #[must_use]
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
                remote: None,
            })
            .collect()
    }

    /// The mailbox handing what it cannot deliver in this process to `remote`, its shell's way to other processes.
    #[must_use]
    pub fn with_remote(mut self, remote: Sender<RemoteMessage>) -> Self {
        self.remote = Some(remote);
        self
    }

    /// The partition the mailbox belongs to.
    #[must_use]
    pub fn id(&self) -> PartitionId {
        self.id
    }

    /// A sender to partition `to`, `None` for one outside the mesh.
    #[must_use]
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
            return;
        }
        if let (Some(remote), PartitionMessage::Register { write, batch, .. }) =
            (&self.remote, message)
        {
            // A shell gone is a process stopping; see above.
            drop(remote.send(RemoteMessage::Register {
                home: to,
                write,
                batch,
            }));
        }
    }

    /// Hands the answer of an asking a Reference brought to the shell.
    pub(crate) fn answer_remote(
        &self,
        write: u64,
        index: usize,
        answer: Result<Answer, PartitionError>,
    ) {
        if let Some(remote) = &self.remote {
            // A shell gone is a process stopping; see above.
            drop(remote.send(RemoteMessage::Answered {
                write,
                index,
                answer,
            }));
        }
    }
}
