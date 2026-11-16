/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commands the frontend alone sends the system group (`PROTOCOL.md` §11.12): the two halves of a client's proof, the
//! registry's Session, Check and Ended, and the catalog's Declare and Defaults.

use std::future::Future;

use compio::runtime::spawn;
use ultragraphdb_common_role::{Answer, Push};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, COMMAND_HEADER_SIZE, Challenge, ChallengeView, Check, CheckView,
    Command, DeclareView, DefaultsView, DropKind, EndedView, ErrorReply, FrameView, Session,
    SessionView, Verify, VerifyView,
};

use crate::types::system_asked::DropStep;
use crate::types::system_session::{SystemSession, header_of};

/// Where a Declare's reply holds `version` and `commit_version` in its command header (`PROTOCOL.md` §13.27).
const DECLARE_VERSION_AT: usize = 32;
const DECLARE_COMMIT_VERSION_AT: usize = 40;

impl SystemSession {
    /// A Challenge: the principal's salt and iteration count, or an unknown name's (`PROTOCOL.md` §13.8).
    pub(super) async fn challenge(
        &self,
        stream: u64,
        frame: &FrameView<'_>,
    ) -> Result<Answer, ErrorReply> {
        let asked = ChallengeView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let name = asked.name().to_string();
        let (iterations, salt) = self.shared.owner.challenge(name.clone()).await?;
        let reply = Challenge::request(name).with_salt(iterations, salt);
        self.built(&reply, stream, Command::Challenge)
    }

    /// A Verify: whether the proof holds, the principal, and when it holds the ServerSignature (`PROTOCOL.md` §13.9). A proof
    /// that does not hold is answered with the request's own body.
    pub(super) async fn verify(
        &self,
        stream: u64,
        frame: &FrameView<'_>,
    ) -> Result<Answer, ErrorReply> {
        let asked = VerifyView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let (name, auth_message) = (asked.name().to_string(), asked.auth_message().to_string());
        let proof = *asked.proof();
        let verified = self
            .shared
            .owner
            .verify(name.clone(), auth_message.clone(), proof)
            .await?;
        let request = Verify::request(name, auth_message, proof);
        let reply = match verified {
            Some((principal, signature)) => request.with_verified(principal, signature),
            None => request,
        };
        self.built(&reply, stream, Command::Verify)
    }

    /// A Session, answered with its own bytes once the registration is durable (`PROTOCOL.md` §13.23); pushed, so the
    /// Checks behind it are answered meanwhile.
    pub(super) fn session(&self, stream: u64, frame: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let asked = SessionView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let reply = Session::new(asked.session(), asked.principal());
        let owner = self.shared.owner.clone();
        let header = header_of(self.shared.cluster.id(), stream, Command::Session);
        let session = u128::from_be_bytes(asked.session());
        let principal = asked.principal();
        self.pushed_later(stream, async move {
            owner.session(session, principal).await?;
            reply
                .frame(header)
                .map_err(|rule| ErrorReply::protocol(&rule))
        });
        Ok(Answer::Open)
    }

    /// A Check, answered whole with each outcome filled (`PROTOCOL.md` §13.24): the receiver refuses a check holding an
    /// outcome, marking nothing.
    pub(super) async fn check(
        &mut self,
        stream: u64,
        frame: &FrameView<'_>,
    ) -> Result<Answer, ErrorReply> {
        let asked = CheckView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        asked
            .check_request()
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        let checks = (0..asked.len())
            .filter_map(|index| asked.get(index))
            .collect();
        let connection = self.connection().await?;
        let answered = self.shared.owner.check(connection, checks).await?;
        self.built(&Check::new(answered), stream, Command::Check)
    }

    /// An Ended, answered by no frame; a body its rules refuse is dropped unanswered (`PROTOCOL.md` §13.25).
    pub(super) async fn ended(&mut self, frame: &FrameView<'_>) -> Answer {
        let sessions: Vec<u128> = match EndedView::from_frame(frame) {
            Ok(asked) => asked
                .sessions()
                .iter()
                .map(|session| u128::from_be_bytes(*session))
                .collect(),
            Err(rule) => {
                tracing::warn!(%rule, "an Ended was refused");
                return Answer::Done;
            }
        };
        match self.connection().await {
            Ok(connection) => self.shared.owner.ended(connection, sessions),
            Err(refusal) => {
                tracing::warn!(reason = refusal.message(), "an Ended found no group");
            }
        }
        Answer::Done
    }

    /// A Declare, its stream held open until the group answers it (`PROTOCOL.md` §13.27): with every field as sent, `version`
    /// and `commit_version` filled.
    pub(super) fn declare(&self, stream: u64, frame: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let asked = DeclareView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let header = header_of(self.shared.cluster.id(), stream, Command::Declare);
        let command_header = *frame.header().command_header();
        let body = frame.body().to_vec();
        let owner = self.shared.owner.clone();
        let tenant = asked.tenant();
        let built_on = asked.built_on();
        let answer = match asked.drop_kind() {
            None => {
                let declared = (0..asked.len())
                    .filter_map(|index| asked.declaration(index))
                    .collect();
                Asked::Version(built_on, declared)
            }
            Some(kind) => Asked::Drop(drop_step(kind, asked.target())),
        };
        self.pushed_later(stream, async move {
            let (version, commit_version) = match answer {
                Asked::Version(base, declared) => {
                    owner.declare_version(tenant, base, declared).await?
                }
                Asked::Drop(step) => (owner.drop_version(tenant, step).await?, 0),
            };
            let mut filled = command_header;
            put_u64(&mut filled, DECLARE_VERSION_AT, version);
            put_u64(&mut filled, DECLARE_COMMIT_VERSION_AT, commit_version);
            header
                .with_command_header(filled)
                .encode_frame(&body)
                .map_err(|rule| ErrorReply::protocol(&rule))
        });
        Ok(Answer::Open)
    }

    /// A Defaults, answered across frames within the requester's `max_frame` (`PROTOCOL.md` §13.28).
    pub(super) async fn defaults(
        &self,
        stream: u64,
        frame: &FrameView<'_>,
    ) -> Result<Answer, ErrorReply> {
        let asked = DefaultsView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let reply = self.shared.owner.defaults(asked.tenant()).await?;
        let frames = reply
            .frames(
                header_of(self.shared.cluster.id(), stream, Command::Defaults),
                self.max_frame,
            )
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        Ok(self.answer_frames(stream, frames))
    }

    /// `body`'s frame on `stream`, as the reply.
    fn built<B: BodyBuilder>(
        &self,
        body: &B,
        stream: u64,
        command: Command,
    ) -> Result<Answer, ErrorReply> {
        body.frame(header_of(self.shared.cluster.id(), stream, command))
            .map(Answer::Reply)
            .map_err(|rule| ErrorReply::protocol(&rule))
    }

    /// Runs `answering` in a task of its own and pushes what it comes to on `stream`: its frame, or its refusal. A connection
    /// gone has no one to tell; what was asked of the group stands.
    fn pushed_later(
        &self,
        stream: u64,
        answering: impl Future<Output = Result<Vec<u8>, ErrorReply>> + 'static,
    ) {
        let pushes = self.pushes.clone();
        spawn(async move {
            let push = match answering.await {
                Ok(frame) => Push::Frame(frame),
                Err(refusal) => Push::Refuse(refusal),
            };
            // A session gone has dropped its receiver: no one is left to tell.
            drop(pushes.unbounded_send((stream, push)));
        })
        .detach();
    }
}

/// What a Declare asks the group for.
enum Asked {
    /// A version of these declarations built on this one.
    Version(u64, Vec<ultragraphdb_protocol_wire::Declared>),
    /// The version a Drop calls for.
    Drop(DropStep),
}

/// The catalog version a Drop of `kind` calls for: a predicate's removal, the initial catalog, or none for a graph's or a
/// tenant's data (`PROTOCOL.md` §13.27).
fn drop_step(kind: DropKind, target: Option<&str>) -> DropStep {
    match kind {
        DropKind::Predicate => DropStep::Predicate(target.unwrap_or_default().to_string()),
        DropKind::Graph | DropKind::TenantData => DropStep::Kept,
        DropKind::Tenant => DropStep::Everything,
    }
}

/// Writes `value` little-endian at `at` of `header`, as a command header holds its fields.
fn put_u64(header: &mut [u8; COMMAND_HEADER_SIZE], at: usize, value: u64) {
    if let Some(field) = header.get_mut(at..at + 8) {
        field.copy_from_slice(&value.to_le_bytes());
    }
}
