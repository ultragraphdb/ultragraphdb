/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the owner answers while one of its logs syncs (design D17): the asks on a commit's path, from what is durable,
//! which a sync changes nothing of until it returns. Every other ask waits for the sync's end, in the order it came.

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::pin::pin;

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use futures::future::{Either, select};
use ultragraphdb_cluster_metadata::MetadataReads;
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};

use crate::types::asked::Asked;
use crate::types::owner::ASKS_PER_TURN_MAX;
use crate::types::tenant_facts::TenantFacts;

/// The `detail` of code 21 for a write another principal's session may not make (`PROTOCOL.md` §11.3).
const ACTION_WRITE: u64 = 2;

/// What a sync's wait answers from: the metadata group's reads, the sessions with a commit in flight, and the last
/// durable close.
pub(super) struct SyncReads<'a> {
    pub(super) reads: MetadataReads<'a>,
    pub(super) in_flight: &'a mut BTreeMap<u128, bool>,
    pub(super) closed: u64,
}

/// Waits on `sync` while answering what `inbox` brings: the asks on a commit's path at once, from `answering`, and every
/// other ask kept in `waiting`, in order, for the turn after the sync. At most [`ASKS_PER_TURN_MAX`] are taken, the rest
/// staying in the channel; an inbox every handle left ends the taking, not the sync.
pub(super) async fn while_syncing<F: Future>(
    sync: F,
    inbox: &mut UnboundedReceiver<Asked>,
    mut answering: SyncReads<'_>,
    waiting: &mut VecDeque<Asked>,
) -> F::Output {
    let mut sync = pin!(sync);
    for _ in 0..ASKS_PER_TURN_MAX {
        match select(sync.as_mut(), inbox.next()).await {
            Either::Left((done, _)) => return done,
            Either::Right((Some(asked), _)) => {
                if let Some(later) = answer_now(asked, &mut answering) {
                    waiting.push_back(later);
                }
            }
            Either::Right((None, _)) => break,
        }
    }
    sync.await
}

/// Answers `asked` when it is on a commit's path, which reads what is durable and changes no log; hands back any other.
/// An asker gone has no one to tell, so every send's failure is dropped.
fn answer_now(asked: Asked, answering: &mut SyncReads<'_>) -> Option<Asked> {
    let reads = answering.reads;
    match asked {
        Asked::TenantFacts { tenant, reply } => {
            let _unheard: Result<(), TenantFacts> =
                reply.send(tenant_facts(reads, answering.closed, tenant));
        }
        Asked::CheckSession {
            session,
            principal,
            reply,
        } => drop(reply.send(check(reads, session, principal))),
        Asked::BeginCommit {
            session,
            principal,
            writes,
            reply,
        } => drop(reply.send(begin_commit(
            reads,
            answering.in_flight,
            (session, principal, writes),
        ))),
        Asked::CommitEnded { session } => {
            let _was = answering.in_flight.remove(&session);
        }
        Asked::SessionHeld { session, reply } => {
            let held = reads.state().session_principal(session).is_ok();
            let _unheard: Result<(), bool> = reply.send(held);
        }
        // Each of these writes a log, reads the sequencer or answers what a sync may change: it waits for the sync's end.
        other @ (Asked::Join { .. }
        | Asked::Members { .. }
        | Asked::Placement { .. }
        | Asked::Catalog { .. }
        | Asked::Lease { .. }
        | Asked::CloseRequest { .. }
        | Asked::Subscribe { .. }
        | Asked::NextClose { .. }
        | Asked::Login(_)
        | Asked::RegisterSession { .. }
        | Asked::Map { .. }
        | Asked::SchemaId { .. }
        | Asked::Describe { .. }
        | Asked::Declare { .. }
        | Asked::DropCatalog { .. }
        | Asked::Report { .. }
        | Asked::RecordDefault { .. }) => return Some(other),
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

/// The owner check: `session` held by the registry, its eviction appended or not, and `principal`'s.
pub(super) fn check(
    reads: MetadataReads<'_>,
    session: u128,
    principal: u64,
) -> Result<(), ErrorReply> {
    let evicted = || {
        ErrorReply::new(
            ErrorCode::SessionEvicted,
            "the registry evicted this session, or never registered it",
        )
    };
    let held = reads
        .state()
        .session_principal(session)
        .map_err(|_| evicted())?;
    // A registration of this turn may evict it before its sync, and the close carrying that eviction could then reach the
    // partition before a commit checked here.
    if !reads.holds_session(session) {
        return Err(evicted());
    }
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
    in_flight: &mut BTreeMap<u128, bool>,
    asked: (u128, u64, bool),
) -> Result<(), ErrorReply> {
    let (session, principal, writes) = asked;
    check(reads, session, principal)?;
    if in_flight.contains_key(&session) {
        return Err(ErrorReply::new(
            ErrorCode::SessionBusy,
            "the session has a commit in flight",
        ));
    }
    let _absent = in_flight.insert(session, writes);
    Ok(())
}
