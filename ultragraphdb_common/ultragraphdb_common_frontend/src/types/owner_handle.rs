/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::{mpsc, oneshot};
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::{Catalog, ErrorCode, ErrorReply, Join, Members, Placement};

use crate::types::asked::Asked;

/// An endpoint core's way to the owner: every question goes by message, and nothing the owner decides is kept here.
///
/// The channel is unbounded because each connection waits for its answer before it asks again, so the connections' own
/// bound bounds it.
#[derive(Debug, Clone)]
pub struct OwnerHandle {
    inbox: mpsc::UnboundedSender<Asked>,
}

impl OwnerHandle {
    pub(crate) fn new(inbox: mpsc::UnboundedSender<Asked>) -> Self {
        Self { inbox }
    }

    /// Asks the owner what `build` says and waits for its answer; `None` when the owner stopped.
    async fn ask<T>(&self, build: impl FnOnce(oneshot::Sender<T>) -> Asked) -> Option<T> {
        let (reply, answer) = oneshot::channel();
        self.inbox.unbounded_send(build(reply)).ok()?;
        answer.await.ok()
    }

    /// As [`OwnerHandle::ask`], an owner that stopped refused as unavailable.
    async fn ask_or_refuse<T>(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<T, ErrorReply>>) -> Asked,
    ) -> Result<T, ErrorReply> {
        self.ask(build).await.unwrap_or_else(|| Err(stopped()))
    }

    /// Registers `join`.
    ///
    /// # Errors
    ///
    /// The refusal of [`crate::Membership::join`], or unavailable when the owner stopped.
    pub async fn join(&self, join: Join) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Join { join, reply })
            .await
    }

    /// Who joined; `None` when the owner stopped.
    pub async fn members(&self) -> Option<Members> {
        self.ask(|reply| Asked::Members { reply }).await
    }

    /// The placement map at `epoch`, 0 for the latest.
    ///
    /// # Errors
    ///
    /// Unavailable when the owner stopped.
    pub async fn placement(&self, epoch: u64) -> Result<Placement, ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Placement { epoch, reply })
            .await
    }

    /// A catalog version of `namespace`, `version` 0 for the current one.
    ///
    /// # Errors
    ///
    /// Code 7 for a version the namespace never held, or unavailable when the owner stopped.
    pub async fn catalog(&self, namespace: u64, version: u64) -> Result<Catalog, ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Catalog {
            namespace,
            version,
            reply,
        })
        .await
    }

    /// Leases `size` sequences of type `prefix` for `partition`: the block's first sequence, answered once durable.
    ///
    /// # Errors
    ///
    /// Code 7 naming the maximum for a size above it, or unavailable when the owner stopped.
    pub async fn lease(&self, prefix: u16, partition: u32, size: u64) -> Result<u64, ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Lease {
            prefix,
            partition,
            size,
            reply,
        })
        .await
    }

    /// Asks for the close of `epoch` as soon as the sequencer may.
    ///
    /// # Errors
    ///
    /// Code 7 for an epoch past the open one, or unavailable when the owner stopped.
    pub async fn close_request(&self, epoch: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::CloseRequest { epoch, reply })
            .await
    }

    /// Whether a subscriber holding the closes through `from` may start.
    ///
    /// # Errors
    ///
    /// Code 1 naming both epochs for a `from` above the latest close, or unavailable when the owner stopped.
    pub async fn subscribe(&self, from: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Subscribe { from, reply })
            .await
    }

    /// The durable close after `after`, once there is one.
    ///
    /// # Errors
    ///
    /// Code 1 when that close is no longer held, or unavailable when the owner stopped.
    pub async fn next_close(&self, after: u64) -> Result<Close, ErrorReply> {
        self.ask_or_refuse(|reply| Asked::NextClose { after, reply })
            .await
    }

    /// The principal holding `name`, and its verifier; `None` when none does or the owner stopped.
    pub async fn verifier(&self, name: String) -> Option<(u64, ScramVerifier)> {
        self.ask(|reply| Asked::Verifier { name, reply })
            .await
            .flatten()
    }

    /// Registers `session` for `principal`, answered once durable.
    ///
    /// # Errors
    ///
    /// Code 7 naming what refused it, or unavailable when the owner stopped.
    pub async fn register_session(&self, session: u128, principal: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::RegisterSession {
            session,
            principal,
            reply,
        })
        .await
    }

    /// The owner check of a request through `session` by `principal`.
    ///
    /// # Errors
    ///
    /// Code 12 for a session the registry does not hold, code 21 `detail` 2 for another principal's, or unavailable
    /// when the owner stopped.
    pub async fn check_session(&self, session: u128, principal: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::CheckSession {
            session,
            principal,
            reply,
        })
        .await
    }
}

fn stopped() -> ErrorReply {
    ErrorReply::new(ErrorCode::Unavailable, "the frontend's owner stopped")
}
