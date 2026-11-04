/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures::channel::oneshot;
use ultragraphdb_common_role::{Answer, Session};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};

use crate::types::core_message::{CoreMessage, Pushes};

/// One connection at a partition's role address: every request goes to its core's task, which owns the partition.
#[derive(Debug)]
pub struct StorageSession {
    connection: u64,
    core: UnboundedSender<CoreMessage>,
    pushes: Pushes,
    pushed: UnboundedReceiver<(u64, Vec<u8>)>,
}

impl StorageSession {
    /// The session of `connection`, handing its requests to `core`.
    #[must_use]
    pub fn new(connection: u64, core: UnboundedSender<CoreMessage>) -> Self {
        let (pushes, pushed) = unbounded();
        Self {
            connection,
            core,
            pushes,
            pushed,
        }
    }
}

impl Session for StorageSession {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let (done, answer) = oneshot::channel();
        let handed = self.core.unbounded_send(CoreMessage::Request {
            connection: self.connection,
            stream,
            frame,
            done,
            pushes: self.pushes.clone(),
        });
        let stopped = || {
            Answer::Refuse(ErrorReply::new(
                ErrorCode::Unavailable,
                "the partition stopped",
            ))
        };
        if handed.is_err() {
            return stopped();
        }
        answer.await.unwrap_or_else(|_| stopped())
    }

    async fn next_push(&mut self) -> (u64, Vec<u8>) {
        match self.pushed.next().await {
            Some(pushed) => pushed,
            // This session holds a sender, so its channel never ends while it waits here.
            None => std::future::pending().await,
        }
    }

    fn cancelled(&mut self, stream: u64) {
        // A core gone holds nothing to abort.
        drop(self.core.unbounded_send(CoreMessage::Cancelled {
            connection: self.connection,
            stream,
        }));
    }
}

impl Drop for StorageSession {
    fn drop(&mut self) {
        // A core gone holds nothing to abort.
        drop(self.core.unbounded_send(CoreMessage::Closed {
            connection: self.connection,
        }));
    }
}
