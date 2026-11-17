/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a core asks the system group on a stream of its own, each answered by its frames.

use futures::channel::oneshot;
use ultragraphdb_core_ids::Version;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Catalog, CatalogFrames, Challenge, ChallengeView, Declare, DeclareView,
    Declared, Defaults, DefaultsFrames, DropKind, ErrorReply, FrameView, Header, MAX_FRAME_SIZE,
    Session, Verify, VerifyView,
};

use super::{Ask, SystemLink, unavailable};

impl SystemLink {
    /// The frames answering `body`, asked on a stream of its own, read until the one flagged last.
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer.
    pub(super) async fn ask<B: BodyBuilder>(&self, body: &B) -> Result<Vec<Vec<u8>>, ErrorReply> {
        let (answer, sender) = {
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
                    frames: Vec::new(),
                    reply,
                },
            );
            (answer, state.queue(frame))
        };
        if let Some(sender) = sender {
            sender.wake();
        }
        answer.await.map_err(|_| {
            unavailable("the connection to the system group closed under the request")
        })?
    }

    /// The cluster the link's frames name.
    pub(super) fn cluster(&self) -> u64 {
        self.state.borrow().cluster
    }

    /// The one frame answering `body`.
    async fn ask_one<B: BodyBuilder>(&self, body: &B) -> Result<Vec<u8>, ErrorReply> {
        self.ask(body)
            .await?
            .into_iter()
            .next_back()
            .ok_or_else(|| unavailable("an answer of no frame"))
    }

    /// Registers `session` for `principal` by a Session, once durable (`PROTOCOL.md` §13.23).
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer.
    pub async fn session(&self, session: [u8; 16], principal: u64) -> Result<(), ErrorReply> {
        self.ask_one(&Session::new(session, principal))
            .await
            .map(drop)
    }

    /// The salt and iteration count of `name` by a Challenge (`PROTOCOL.md` §13.8).
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer.
    pub async fn challenge(&self, name: &str) -> Result<(u32, Vec<u8>), ErrorReply> {
        let frame = self.ask_one(&Challenge::request(name)).await?;
        let view = self.read(&frame)?;
        let challenge =
            ChallengeView::from_frame(&view).map_err(|rule| ErrorReply::protocol(&rule))?;
        Ok((challenge.iterations(), challenge.salt().to_vec()))
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
        let frame = self
            .ask_one(&Verify::request(name, auth_message, *proof))
            .await?;
        let view = self.read(&frame)?;
        let verify = VerifyView::from_frame(&view).map_err(|rule| ErrorReply::protocol(&rule))?;
        Ok(verify
            .signature()
            .filter(|_| verify.verified())
            .map(|signature| (verify.principal(), *signature)))
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
        let frame = self.ask_one(&body).await?;
        let view = self.read(&frame)?;
        let reply = DeclareView::from_frame(&view).map_err(|rule| ErrorReply::protocol(&rule))?;
        let answered = (reply.version(), reply.commit_version());
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
        let frame = self
            .ask_one(&Declare::drop(tenant, principal, kind, target))
            .await?;
        let view = self.read(&frame)?;
        let reply = DeclareView::from_frame(&view).map_err(|rule| ErrorReply::protocol(&rule))?;
        let version = reply.version();
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
        let frames = self.ask(&Defaults::request(tenant)).await?;
        let mut read = DefaultsFrames::new();
        for frame in &frames {
            let view = self.read(frame)?;
            read.push(&view)
                .map_err(|rule| ErrorReply::protocol(&rule))?;
        }
        Ok(read.predicates().to_vec())
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
        let frames = self.ask(request).await?;
        let mut read = CatalogFrames::new();
        for frame in &frames {
            let view = self.read(frame)?;
            read.push(&view)
                .map_err(|rule| ErrorReply::protocol(&rule))?;
        }
        let version = read
            .view()
            .ok_or_else(|| unavailable("a Catalog answer of no frame"))?;
        Ok((version.version(), version.epoch(), version.bytes().to_vec()))
    }

    /// `frame` validated as a frame of the link's cluster.
    pub(super) fn read<'a>(&self, frame: &'a [u8]) -> Result<FrameView<'a>, ErrorReply> {
        FrameView::validate(frame, self.cluster(), MAX_FRAME_SIZE)
            .map_err(|rule| ErrorReply::protocol(&rule))
    }
}
