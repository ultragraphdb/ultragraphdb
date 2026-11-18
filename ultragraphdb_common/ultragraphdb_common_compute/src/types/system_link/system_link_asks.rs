/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a core asks the system group on a stream of its own, each answered by its frames.

use futures::channel::oneshot;
use ultragraphdb_core_ids::Version;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Catalog, CatalogFrames, Challenge, ChallengeView, Declare, DeclareView,
    Declared, Defaults, DefaultsFrames, DropKind, ErrorReply, FrameView, Header, MAX_FRAME_SIZE,
    Session, SessionView, Verify, VerifyView,
};

use super::{Ask, SystemLink, system_link_tasks, unavailable};

impl SystemLink {
    /// The frames answering `body`, asked on a stream of its own, read until the one flagged last, and read by `read`: what
    /// it reads. A refusal by `read` closes the connection the frames came on (`PROTOCOL.md` §2.6, §13).
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer, or `read` refuses it.
    pub(super) async fn ask<B: BodyBuilder, T>(
        &self,
        body: &B,
        read: impl FnOnce(u64, &[Vec<u8>]) -> Result<T, ProtocolError>,
    ) -> Result<T, ErrorReply> {
        let (answer, sender, connection) = {
            let mut state = self.state.borrow_mut();
            if !state.open {
                return Err(unavailable("the connection to the system group is closed"));
            }
            let stream = state.stream();
            let header = Header::new(state.cluster, B::COMMAND).with_stream(stream);
            let frame = body
                .frame(header)
                .map_err(|rule| ErrorReply::protocol(&rule))?;
            let (reply, answer) = oneshot::channel();
            let _none = state.asks.insert(
                stream,
                Ask {
                    command: B::COMMAND,
                    frames: Vec::new(),
                    reply,
                },
            );
            (answer, state.queue(frame), state.connection)
        };
        if let Some(sender) = sender {
            sender.wake();
        }
        let frames = answer.await.map_err(|_| {
            unavailable("the connection to the system group closed under the request")
        })??;
        read(self.cluster(), &frames)
            .map_err(|rule| system_link_tasks::refuse(self, connection, &rule))
    }

    /// The cluster the link's frames name.
    pub(super) fn cluster(&self) -> u64 {
        self.state.borrow().cluster
    }

    /// Registers `session` for `principal` by a Session, once durable (`PROTOCOL.md` §13.23).
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer.
    pub async fn session(&self, session: [u8; 16], principal: u64) -> Result<(), ErrorReply> {
        self.ask(&Session::new(session, principal), |cluster, frames| {
            SessionView::from_frame(&last(cluster, frames)?).map(drop)
        })
        .await
    }

    /// The salt and iteration count of `name` by a Challenge (`PROTOCOL.md` §13.8).
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer.
    pub async fn challenge(&self, name: &str) -> Result<(u32, Vec<u8>), ErrorReply> {
        self.ask(&Challenge::request(name), |cluster, frames| {
            let view = last(cluster, frames)?;
            let challenge = ChallengeView::from_frame(&view)?;
            Ok((challenge.iterations(), challenge.salt().to_vec()))
        })
        .await
    }

    /// Whether `proof` holds for `name` over `auth_message` by a Verify: the principal and the ServerSignature when it
    /// does (`PROTOCOL.md` §13.9).
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer.
    pub async fn verify(
        &self,
        name: &str,
        auth_message: &str,
        proof: &[u8; 32],
    ) -> Result<Option<(u64, [u8; 32])>, ErrorReply> {
        self.ask(
            &Verify::request(name, auth_message, *proof),
            |cluster, frames| {
                let view = last(cluster, frames)?;
                let verify = VerifyView::from_frame(&view)?;
                Ok(verify
                    .signature()
                    .filter(|_| verify.verified())
                    .map(|signature| (verify.principal(), *signature)))
            },
        )
        .await
    }

    /// A catalog version of `declared` built on `built_on` for `tenant` by a Declare of kind 1: the version and the first
    /// commit version of its effective epoch (`PROTOCOL.md` §13.27). The view learns the version.
    ///
    /// # Errors
    ///
    /// The group's refusal, code 2 of kind 5 for a stale base among them; code 17 when the link is closed or closes first.
    pub async fn declare(
        &self,
        tenant: u64,
        built_on: u64,
        principal: u64,
        declared: Vec<Declared>,
    ) -> Result<(u64, u64), ErrorReply> {
        let body = Declare::declarations(tenant, built_on, principal, declared);
        let answered = self.ask(&body, declared_version).await?;
        // The version takes effect at the epoch of the first commit version it answers (`PROTOCOL.md` §11.13, *Its epoch*).
        let epoch = Version::from_u64(answered.1).epoch().get();
        self.state
            .borrow_mut()
            .view
            .declared(tenant, answered.0, epoch);
        self.notify();
        Ok(answered)
    }

    /// The catalog version a Drop of `kind` naming `target` calls for, by a Declare of kind 2: 0 when it changes nothing in
    /// the catalog (`PROTOCOL.md` §13.27).
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer.
    pub async fn drop_catalog(
        &self,
        tenant: u64,
        principal: u64,
        kind: DropKind,
        target: Option<String>,
    ) -> Result<u64, ErrorReply> {
        let (version, _commit_version) = self
            .ask(
                &Declare::drop(tenant, principal, kind, target),
                declared_version,
            )
            .await?;
        if version != 0 {
            // Committed as pending: no epoch is assigned to it yet (`PROTOCOL.md` §13.27).
            self.state.borrow_mut().view.declared(tenant, version, 0);
            self.notify();
        }
        Ok(version)
    }

    /// The predicates `tenant`'s default-entry records name, in order, read across Defaults frames (`PROTOCOL.md` §13.28).
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer.
    pub async fn defaults(&self, tenant: u64) -> Result<Vec<String>, ErrorReply> {
        self.ask(&Defaults::request(tenant), |cluster, frames| {
            let mut read = DefaultsFrames::new();
            for frame in frames {
                read.push(&FrameView::validate(frame, cluster, MAX_FRAME_SIZE)?)?;
            }
            Ok(read.predicates().to_vec())
        })
        .await
    }

    /// The catalog version a client reads at `epoch` and its bytes, by a Catalog of `version` 0 naming `epoch`, read across
    /// Catalog frames (`PROTOCOL.md` §13.7).
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer.
    pub async fn catalog_at(&self, tenant: u64, epoch: u64) -> Result<(u64, Vec<u8>), ErrorReply> {
        self.catalog_read(&Catalog::request_at(tenant, epoch))
            .await
            .map(|(version, _effective, bytes)| (version, bytes))
    }

    /// The version `request` asks for, the epoch it takes effect at, 0 while pending, and its bytes, read across Catalog
    /// frames.
    pub(super) async fn catalog_read(
        &self,
        request: &Catalog,
    ) -> Result<(u64, u64, Vec<u8>), ErrorReply> {
        self.ask(request, |cluster, frames| {
            let mut read = CatalogFrames::new();
            for frame in frames {
                read.push(&FrameView::validate(frame, cluster, MAX_FRAME_SIZE)?)?;
            }
            Ok(read
                .view()
                .map(|version| (version.version(), version.epoch(), version.bytes().to_vec())))
        })
        .await?
        .ok_or_else(|| unavailable("a Catalog answer of no frame"))
    }
}

/// The last of `frames`, an answer's, validated as a frame of `cluster`: an answer holds its last frame, and an empty one is
/// refused as short.
pub(super) fn last(cluster: u64, frames: &[Vec<u8>]) -> Result<FrameView<'_>, ProtocolError> {
    FrameView::validate(
        frames.last().map_or(&[][..], Vec::as_slice),
        cluster,
        MAX_FRAME_SIZE,
    )
}

/// The version and commit version a Declare's reply answers.
fn declared_version(cluster: u64, frames: &[Vec<u8>]) -> Result<(u64, u64), ProtocolError> {
    let reply = DeclareView::from_frame(&last(cluster, frames)?)?;
    Ok((reply.version(), reply.commit_version()))
}
