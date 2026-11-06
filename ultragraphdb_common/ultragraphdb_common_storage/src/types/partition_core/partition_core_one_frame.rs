/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphNameRef, NamedNodeRef};
use ultragraphdb_cluster_partition::{BoundKey, BoundStep, PartitionError};
use ultragraphdb_common_role::Answer;
use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, DropCommand, DropCommandView, DropKind, ErrorCode, ErrorReply,
    FrameView, MAX_FRAME_SIZE, RequestTermView, WriteChange,
};
use ultragraphdb_store_apply::DropRequest;
use ultragraphdb_transactions_commit::{CommitError, CommitErrorEnum, StoredReply};
use ultragraphdb_transactions_txn::{FrameChanges, FrameTerms};

use crate::types::core_message::Pushes;
use crate::types::one_frame_view::OneFrameView;
use crate::types::partition_core::partition_core_frames::{built, last};
use crate::types::partition_core::{
    Committing, OneFrameAt, OneFrameRetry, ParkedWork, PartitionCore, RequestStep,
};
use crate::utils::commit_body::commit_body;
use crate::utils::error_reply::error_reply;
use crate::utils::operation_of::operation_of;

/// How many times a one-frame write runs again on a conflict over what it tested, before code 2 reaches its client (`PROTOCOL.md`
/// §6.6): each run reads a newer watermark, so only as many as this many commits landing in its way refuse it.
pub(super) const ONE_FRAME_ATTEMPTS: u32 = 8;

/// What a run of a one-frame write came to.
#[derive(Clone, Copy)]
enum OneFrameStep {
    /// It waits for its references' answers, and goes on from there.
    Waiting(OneFrameAt),
    /// It is in the commit step, under this ticket.
    InCommit(u64),
}

fn protocol(rule: &ultragraphdb_error_protocol::ProtocolError) -> ErrorReply {
    ErrorReply::protocol(rule)
}

/// The identity a one-frame write holds its execution under, which no Scan names: its connection and stream.
fn identity_of((connection, stream): BoundKey) -> [u8; 16] {
    (u128::from(connection) << 64 | u128::from(stream)).to_be_bytes()
}

impl PartitionCore<'_> {
    /// A DirectWrite or ConditionalWrite: a resend answered from the session table, else executed from its frame in one turn, its
    /// references' wait apart (`PROTOCOL.md` §6.6).
    pub(super) fn one_frame(&mut self, key: BoundKey, view: &FrameView<'_>) -> RequestStep {
        let step = OneFrameView::of(view)
            .map_err(|rule| protocol(&rule))
            .and_then(|one| self.start_one_frame(key, &one));
        match step {
            Ok(Started::Stored(answer)) => RequestStep::Answered(answer),
            Ok(Started::Step(step)) => step.into(),
            Err(refusal) => RequestStep::Answered(Answer::Refuse(refusal)),
        }
    }

    /// The session table's word on it, or its first run.
    fn start_one_frame(
        &mut self,
        key: BoundKey,
        one: &OneFrameView<'_>,
    ) -> Result<Started, ErrorReply> {
        let (session, request) = (one.session(), one.request());
        let stored = self
            .host
            .resend_of(session, request)
            .map_err(|refusal| error_reply(&refusal))?;
        if let Some(stored) = stored {
            let reply = stored_commit(session, request, &stored);
            return built(&reply, last(self.cluster, key.1, Command::Commit)).map(Started::Stored);
        }
        self.bind_one_frame(key, one)?;
        // A run refused after its execution opened ends it, so nothing stays held for a frame already answered.
        self.step_one_frame(key, one, OneFrameAt::Changes)
            .map(Started::Step)
            .inspect_err(|_| self.host.abort_bound(key))
    }

    /// Opens the execution a run holds, at the watermark, and checks what a request named direct may not hold.
    fn bind_one_frame(&mut self, key: BoundKey, one: &OneFrameView<'_>) -> Result<(), ErrorReply> {
        let now = self.now_us();
        if one.is_direct() {
            self.refuse_unique(one)?;
        }
        let _read_version = self
            .host
            .bind(
                key,
                identity_of(key),
                (one.tenant(), one.principal(), one.epoch()),
                now,
            )
            .map_err(|refusal| error_reply(&refusal))?;
        Ok(())
    }

    /// Refuses a DirectWrite inserting a predicate the catalog in effect declares unique: code 7, `detail` the term's index,
    /// naming the predicate and ConditionalWrite (`PROTOCOL.md` §6.6).
    fn refuse_unique(&self, one: &OneFrameView<'_>) -> Result<(), ErrorReply> {
        let catalog = self
            .host
            .partition()
            .applier()
            .catalog(one.tenant())
            .map_err(|refusal| error_reply(&refusal.into()))?;
        let is_unique = |iri: &str| {
            catalog
                .predicates()
                .any(|(_, held)| held.unique() && held.predicate().as_str() == iri)
                || catalog.unresolved(iri).is_some_and(|held| held.unique())
        };
        let inserted = (0..one.change_count())
            .filter_map(|index| one.change(index))
            .filter_map(|change| match change {
                WriteChange::Insert { predicate, .. }
                | WriteChange::InsertEdge { predicate, .. } => Some(predicate),
                WriteChange::Delete { .. }
                | WriteChange::DeleteStatement { .. }
                | WriteChange::DeletePattern { .. }
                | WriteChange::SetProperty { .. }
                | WriteChange::DeleteProperty { .. }
                | WriteChange::Declare(_) => None,
            });
        for predicate in inserted {
            if let Some(RequestTermView::Iri(iri)) = one.terms().get(predicate)
                && is_unique(iri)
            {
                return Err(ErrorReply::new(
                    ErrorCode::Refused,
                    format!(
                        "the predicate {iri} is declared unique, which a ConditionalWrite claims"
                    ),
                )
                .with_detail(predicate.into()));
            }
        }
        Ok(())
    }

    /// Runs `one` from `at`: its changes, then each operation in order, each done at once or waiting for its references' answers,
    /// and then its commit.
    fn step_one_frame(
        &mut self,
        key: BoundKey,
        one: &OneFrameView<'_>,
        at: OneFrameAt,
    ) -> Result<OneFrameStep, ErrorReply> {
        let terms = FrameTerms::of(one.terms()).map_err(|rule| protocol(&rule))?;
        let now = self.now_us();
        let mut next = 0;
        // A frame of operations alone leaves its first subject to its first operation, which the partition checks as a first
        // request's: code 26 for a stale route, so the frontend resends it (`PROTOCOL.md` §6.6).
        if matches!(at, OneFrameAt::Changes | OneFrameAt::ChangesHeard) && one.change_count() > 0 {
            let changes = FrameChanges::of_records(
                one.tenant(),
                one.terms(),
                &terms,
                one.change_count(),
                |index| one.change(index),
            )
            .map_err(|rule| protocol(&rule))?;
            let host = &mut self.host;
            if matches!(at, OneFrameAt::ChangesHeard) {
                host.resume_write(key, &changes, now)
            } else {
                match host.write_bound(key, &changes, now) {
                    Ok(BoundStep::Done) => Ok(()),
                    Ok(BoundStep::Waiting) => {
                        return Ok(OneFrameStep::Waiting(OneFrameAt::ChangesHeard));
                    }
                    Err(refusal) => Err(refusal),
                }
            }
            .map_err(|refusal| error_reply(&refusal))?;
        } else if let OneFrameAt::Operation(index) = at {
            next = index;
        }
        for index in next..one.operation_count() {
            let wire = one
                .operation(index)
                .ok_or_else(|| ErrorReply::new(ErrorCode::Protocol, "an operation not held"))?;
            let operation =
                operation_of(&wire, one.terms(), &terms).map_err(|rule| protocol(&rule))?;
            match self.host.operate_bound(key, operation, now) {
                Ok(BoundStep::Done) => {}
                Ok(BoundStep::Waiting) => {
                    return Ok(OneFrameStep::Waiting(OneFrameAt::Operation(index + 1)));
                }
                Err(refusal) => return Err(error_reply(&refusal)),
            }
        }
        self.host
            .commit_one_frame(key, one.session(), one.request(), now)
            .map(OneFrameStep::InCommit)
            .map_err(|refusal| error_reply(&refusal))
    }

    /// A run in the commit step under `ticket`, its frame kept for the conflict that runs it again.
    pub(super) fn committing_one_frame(
        &mut self,
        key: BoundKey,
        bytes: Vec<u8>,
        attempts: u32,
        ticket: u64,
        pushes: Pushes,
    ) {
        let (session, request) = FrameView::validate(&bytes, self.cluster, MAX_FRAME_SIZE)
            .ok()
            .and_then(|view| {
                OneFrameView::of(&view)
                    .ok()
                    .map(|one| (one.session(), one.request()))
            })
            .unwrap_or((0, 0));
        let _none = self.committing.insert(
            ticket,
            Committing {
                stream: key.1,
                session,
                request,
                pushes,
                retry: Some(OneFrameRetry {
                    connection: key.0,
                    frame: bytes,
                    attempts: attempts.saturating_sub(1),
                }),
                dropped: None,
            },
        );
    }

    /// A parked one-frame write whose references were answered, on its way from `at`.
    pub(super) fn resume_one_frame(
        &mut self,
        key: BoundKey,
        view: &FrameView<'_>,
        at: OneFrameAt,
    ) -> RequestStep {
        let step = OneFrameView::of(view)
            .map_err(|rule| protocol(&rule))
            .and_then(|one| self.step_one_frame(key, &one, at));
        match step {
            Ok(step) => step.into(),
            Err(refusal) => {
                self.host.abort_bound(key);
                RequestStep::Answered(Answer::Refuse(refusal))
            }
        }
    }

    /// `committing`, a one-frame write its round refused with a conflict over what it tested, run again at a newer watermark:
    /// whether it took the run. A run that would wait on a home again leaves the conflict to its client.
    pub(super) fn retry_one_frame(
        &mut self,
        committing: &mut Committing,
        refusal: &PartitionError,
    ) -> bool {
        let Some(retry) = committing.retry.as_mut() else {
            return false;
        };
        if retry.attempts == 0 || !is_conflict(refusal) {
            return false;
        }
        let key = (retry.connection, committing.stream);
        let (attempts, frame) = (retry.attempts, std::mem::take(&mut retry.frame));
        let Ok(view) = FrameView::validate(&frame, self.cluster, MAX_FRAME_SIZE) else {
            return false;
        };
        let Ok(one) = OneFrameView::of(&view) else {
            return false;
        };
        let ran = self
            .bind_one_frame(key, &one)
            .and_then(|()| self.step_one_frame(key, &one, OneFrameAt::Changes));
        match ran {
            Ok(OneFrameStep::InCommit(ticket)) => {
                let pushes = committing.pushes.clone();
                self.committing_one_frame(key, frame, attempts, ticket, pushes);
                true
            }
            Ok(OneFrameStep::Waiting(_)) => {
                self.host.abort_bound(key);
                false
            }
            Err(error) => {
                self.host.abort_bound(key);
                self.push_error(committing.stream, &error, &committing.pushes);
                true
            }
        }
    }

    /// A Drop on `key`, whose Bind of kind 2 named `tenant` and `principal`: its resend answered from the session table, else
    /// committed in its own round (`PROTOCOL.md` §6.5).
    pub(super) fn drop_bound(
        &mut self,
        key: BoundKey,
        (tenant, principal): (u64, u64),
        view: &FrameView<'_>,
        pushes: &Pushes,
    ) -> Result<Answer, ErrorReply> {
        let asked = DropCommandView::from_frame(view).map_err(|rule| protocol(&rule))?;
        if asked.tenant() != tenant {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                format!(
                    "a Drop of tenant {} on a stream bound to tenant {tenant}",
                    asked.tenant()
                ),
            ));
        }
        let session = u128::from_be_bytes(asked.session());
        let request = asked.request();
        let body = asked.to_body().map_err(|rule| protocol(&rule))?;
        let header = last(self.cluster, key.1, Command::Drop);
        let stored = self
            .host
            .resend_of(session, request)
            .map_err(|refusal| error_reply(&refusal))?;
        if let Some(stored) = stored {
            let reply = dropped_reply(body, stored.version(), Some(stored.token()));
            return built(&reply, header);
        }
        let now = self.now_us();
        let ticket = drop_request(&asked).and_then(|drop| {
            self.host
                .commit_drop((tenant, principal), drop, (session, request), now)
                .map_err(|refusal| error_reply(&refusal))
        })?;
        let _none = self.committing.insert(
            ticket,
            Committing {
                stream: key.1,
                session,
                request,
                pushes: pushes.clone(),
                retry: None,
                dropped: Some(body),
            },
        );
        Ok(Answer::Open)
    }

    /// The frame a Drop's outcome answers with.
    pub(super) fn dropped_frame(
        &self,
        stream: u64,
        body: DropCommand,
        version: u64,
        token: Option<&SessionToken>,
    ) -> Result<Vec<u8>, ultragraphdb_error_protocol::ProtocolError> {
        dropped_reply(body, version, token).frame(last(self.cluster, stream, Command::Drop))
    }
}

impl From<OneFrameStep> for RequestStep {
    fn from(step: OneFrameStep) -> Self {
        match step {
            // A one-frame write's tenant is its frame's, so the slot of a Write's stays unused.
            OneFrameStep::Waiting(at) => Self::Parks {
                tenant: 0,
                work: ParkedWork::OneFrame { at },
            },
            OneFrameStep::InCommit(ticket) => Self::InCommit(ticket),
        }
    }
}

/// What a one-frame write's start came to.
enum Started {
    /// Answered from the session table.
    Stored(Answer),
    /// Its first run.
    Step(OneFrameStep),
}

fn stored_commit(
    session: u128,
    request: u64,
    stored: &StoredReply,
) -> ultragraphdb_protocol_wire::Commit {
    commit_body(
        (session, request),
        stored.version(),
        Some(stored.token()),
        stored.results(),
        stored.terms(),
    )
}

/// A Drop's reply: its request with the commit version and, through a session, its token.
fn dropped_reply(body: DropCommand, version: u64, token: Option<&SessionToken>) -> DropCommand {
    let reply = body.with_commit_version(version);
    match token {
        Some(token) => reply.with_token(token.clone()),
        None => reply,
    }
}

/// What a Drop asks to remove, by name.
fn drop_request<'a>(asked: &DropCommandView<'a>) -> Result<DropRequest<'a>, ErrorReply> {
    let target = || -> Result<NamedNodeRef<'a>, ErrorReply> {
        let iri = asked.target().unwrap_or_default();
        NamedNodeRef::new(iri).map_err(|error| {
            ErrorReply::new(
                ErrorCode::Refused,
                format!("the Drop's IRI {iri} is not one: {error}"),
            )
        })
    };
    Ok(match asked.kind() {
        DropKind::Predicate => DropRequest::Predicate(target()?),
        DropKind::Graph => DropRequest::Graph(GraphNameRef::NamedNode(target()?)),
        DropKind::TenantData => DropRequest::Data,
        DropKind::Tenant => DropRequest::Tenant,
    })
}

/// Whether `refusal` is a conflict over what an execution tested, which a run at a newer watermark may pass.
fn is_conflict(refusal: &PartitionError) -> bool {
    use ultragraphdb_cluster_partition::PartitionErrorEnum;
    matches!(
        &refusal.0,
        PartitionErrorEnum::Commit(CommitError(CommitErrorEnum::Conflict { .. }))
    )
}
