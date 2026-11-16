/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the owner answers while one of its logs syncs (design D17): the asks on a commit's path, from what is durable,
//! which a sync changes nothing of until it returns. Every other ask waits for the sync's end, in the order it came.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::pin;

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use futures::future::{Either, select};
use ultragraphdb_cluster_metadata::MetadataReads;
use ultragraphdb_cluster_sequencer::ClockReading;
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};

use crate::types::session_marks::SessionMarks;
use crate::types::system_asked::SystemAsked;
use crate::types::system_group::ASKS_PER_TURN_MAX;
use crate::types::system_group::system_group_sessions::{checked, ended};
use crate::types::tenant_facts::TenantFacts;

/// The `detail` of code 21 for a write another principal's session may not make (`PROTOCOL.md` §11.3).
const ACTION_WRITE: u64 = 2;

/// What a sync's wait answers from: the metadata group's reads, the marks of the requests in flight, and the last
/// durable close.
pub(super) struct SyncReads<'a> {
    pub(super) reads: MetadataReads<'a>,
    pub(super) marks: &'a mut SessionMarks,
    pub(super) closed: u64,
}

/// Waits on `sync` while answering what `inbox` brings: the asks on a commit's path at once, from `answering` at the time
/// `clock` reads as each is taken, and every other ask kept in `waiting`, in order, for the turn after the sync. At most
/// [`ASKS_PER_TURN_MAX`] are taken, the rest staying in the channel; an inbox every handle left ends the taking, not the
/// sync.
pub(super) async fn while_syncing<F: Future>(
    sync: F,
    inbox: &mut UnboundedReceiver<SystemAsked>,
    mut answering: SyncReads<'_>,
    waiting: &mut VecDeque<SystemAsked>,
    clock: &impl Fn() -> ClockReading,
) -> F::Output {
    let mut sync = pin!(sync);
    for _ in 0..ASKS_PER_TURN_MAX {
        match select(sync.as_mut(), inbox.next()).await {
            Either::Left((done, _)) => return done,
            Either::Right((Some(asked), _)) => {
                let now_us = clock().monotonic_us();
                if let Some(later) = answer_now(asked, &mut answering, now_us) {
                    waiting.push_back(later);
                }
            }
            Either::Right((None, _)) => break,
        }
    }
    sync.await
}

/// Answers `asked` at `now_us` when it is on a commit's path, which reads what the registry holds and changes no log, or
/// changes the marks alone; hands back any other. An asker gone has no one to tell, so every send's failure is dropped.
fn answer_now(
    asked: SystemAsked,
    answering: &mut SyncReads<'_>,
    now_us: u64,
) -> Option<SystemAsked> {
    let reads = answering.reads;
    answering.marks.expire(now_us);
    match asked {
        SystemAsked::TenantFacts { tenant, reply } => {
            let _unheard: Result<(), TenantFacts> =
                reply.send(tenant_facts(reads, answering.closed, tenant));
        }
        SystemAsked::CheckSession {
            session,
            principal,
            reply,
        } => drop(reply.send(check(reads, session, principal))),
        SystemAsked::BeginCommit {
            session,
            principal,
            writes,
            reply,
        } => drop(reply.send(begin_commit(
            reads,
            answering.marks,
            (session, principal, writes),
        ))),
        SystemAsked::CommitEnded { session } => answering.marks.end_in_process(session),
        SystemAsked::Check {
            connection,
            checks,
            reply,
        } => {
            let decided = checked(reads, answering.marks, now_us, connection, checks);
            let _unheard: Result<(), Vec<_>> = reply.send(decided);
        }
        SystemAsked::Ended {
            connection,
            sessions,
        } => ended(answering.marks, connection, &sessions),
        SystemAsked::Connect { reply } => {
            let _unheard: Result<(), u64> = reply.send(answering.marks.open());
        }
        SystemAsked::Disconnect { connection } => answering.marks.close(connection, now_us),
        SystemAsked::SessionHeld { session, reply } => {
            let held = reads.state().session_principal(session).is_ok();
            let _unheard: Result<(), bool> = reply.send(held);
        }
        // Each of these writes a log, reads the sequencer or answers what a sync may change: it waits for the sync's end.
        other @ (SystemAsked::Join { .. }
        | SystemAsked::Members { .. }
        | SystemAsked::Placement { .. }
        | SystemAsked::Catalog { .. }
        | SystemAsked::Lease { .. }
        | SystemAsked::CloseRequest { .. }
        | SystemAsked::Subscribe { .. }
        | SystemAsked::NextClose { .. }
        | SystemAsked::Verifier { .. }
        | SystemAsked::RegisterSession { .. }
        | SystemAsked::Map { .. }
        | SystemAsked::SchemaId { .. }
        | SystemAsked::Describe { .. }
        | SystemAsked::Declare { .. }
        | SystemAsked::DropCatalog { .. }
        | SystemAsked::Report { .. }
        | SystemAsked::RecordDefault { .. }
        | SystemAsked::Session { .. }
        | SystemAsked::Defaults { .. }
        | SystemAsked::CatalogAt { .. }
        | SystemAsked::Challenge { .. }
        | SystemAsked::Verify { .. }
        | SystemAsked::LatestClose { .. }) => return Some(other),
    }
    None
}

/// What the gate reads of `tenant`: whether the group holds it, the open epoch after the last durable close `closed`, that
/// close, and its current catalog version.
pub(super) fn tenant_facts(reads: MetadataReads<'_>, closed: u64, tenant: u64) -> TenantFacts {
    let state = reads.state();
    let held = Tenant::new(tenant);
    TenantFacts::new(
        state.holds_tenant(held),
        closed.saturating_add(1),
        closed,
        state.current_version(held),
    )
}

/// The owner check: `session` held by the registry after every command appended, its eviction appended or not, and
/// `principal`'s there.
pub(super) fn check(
    reads: MetadataReads<'_>,
    session: u128,
    principal: u64,
) -> Result<(), ErrorReply> {
    // A registration of this turn may evict it before its sync, and the close carrying that eviction could then reach the
    // partition before a commit checked here.
    let held = reads.session_principal(session).ok_or_else(|| {
        ErrorReply::new(
            ErrorCode::SessionEvicted,
            "the registry evicted this session, or never registered it",
        )
    })?;
    if held == principal {
        return Ok(());
    }
    Err(ErrorReply::new(
        ErrorCode::PermissionDenied,
        "the session is another principal's",
    )
    .with_detail(ACTION_WRITE))
}

/// The owner check of a commit, one in flight per session (`PROTOCOL.md` §11.5): `asked` is the session, the principal,
/// and whether the commit writes.
pub(super) fn begin_commit(
    reads: MetadataReads<'_>,
    marks: &mut SessionMarks,
    asked: (u128, u64, bool),
) -> Result<(), ErrorReply> {
    let (session, principal, writes) = asked;
    check(reads, session, principal)?;
    if marks.is_marked(session) {
        return Err(ErrorReply::new(
            ErrorCode::SessionBusy,
            "the session has a commit in flight",
        ));
    }
    marks.mark(session, None, writes);
    Ok(())
}
