/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The registry asked by Session, Check and Ended (`PROTOCOL.md` §11.5, §13.23 to §13.25): what a check decides, what an
//! Ended ends, and a Session's refusals in their order.

use futures::channel::oneshot::Sender;
use ultragraphdb_cluster_metadata::MetadataReads;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_wire::{CheckKind, CheckOutcome, ErrorCode, ErrorReply, SessionCheck};
use ultragraphdb_traits_log::OrderedLog;

use crate::types::session_marks::SessionMarks;
use crate::types::system_group::system_group_answer::refused_or_stop;
use crate::types::system_group::{Durable, SystemGroup};

/// `checks`, sent on `connection`, each with its outcome, decided in their order at `now_us`: 12 when the registry does not
/// hold the session, 21 when it belongs to another principal, 14 when it is marked or, of kind 1, in the quiet time, and 1
/// otherwise; a check of kind 2 that passes marks its session against `connection`.
pub(super) fn checked(
    reads: MetadataReads<'_>,
    marks: &mut SessionMarks,
    now_us: u64,
    connection: u64,
    checks: Vec<SessionCheck>,
) -> Vec<SessionCheck> {
    marks.expire(now_us);
    let quiet = marks.is_quiet(now_us);
    checks
        .into_iter()
        .map(|check| {
            let session = u128::from_be_bytes(check.session());
            let outcome = outcome_of(reads, marks, quiet, session, &check);
            if outcome == CheckOutcome::Passed && check.kind() == CheckKind::Marks {
                marks.mark(session, Some(connection), true);
            }
            check.with_outcome(outcome)
        })
        .collect()
}

/// The outcome of `check` of `session`, the first that applies of 12, 21 and 14, and 1 otherwise (`PROTOCOL.md` §11.5): a check
/// of kind 1 that would pass answers 14 in the quiet time, since a restart lost the marks of the requests then on their way.
fn outcome_of(
    reads: MetadataReads<'_>,
    marks: &SessionMarks,
    quiet: bool,
    session: u128,
    check: &SessionCheck,
) -> CheckOutcome {
    let Some(held) = reads.session_principal(session) else {
        return CheckOutcome::NotHeld;
    };
    if held != check.principal() {
        return CheckOutcome::OtherPrincipal;
    }
    if marks.is_marked(session) || (quiet && check.kind() == CheckKind::Asks) {
        return CheckOutcome::InFlight;
    }
    CheckOutcome::Passed
}

/// An Ended's `sessions` sent on `connection`: each mark begun on `connection` or on a closed connection ends.
pub(super) fn ended(marks: &mut SessionMarks, connection: u64, sessions: &[u128]) {
    for session in sessions {
        marks.end(*session, connection);
    }
}

impl<M: OrderedLog, S: OrderedLog> SystemGroup<M, S> {
    /// A Session at `now_us`, kept for the turn's sync once registered: refused with Error 1 for a session the registry holds,
    /// code 7 for a principal the group does not hold, and code 14 when the full registry holds no session it may evict now.
    /// An asker gone has no one to tell, so each reply's failed send is dropped.
    pub(super) fn asked_session(
        &mut self,
        session: u128,
        principal: u64,
        now_us: u64,
        reply: Sender<Result<(), ErrorReply>>,
    ) -> Result<(), RoleError> {
        let reads = self.metadata.reads();
        if reads.session_principal(session).is_some() {
            let held = format!("the registry holds session {session:032x}");
            drop(reply.send(Err(ErrorReply::new(ErrorCode::Protocol, held))));
            return Ok(());
        }
        let Some(name) = self
            .metadata
            .state()
            .principal_by_id(principal)
            .map(|held| held.name().to_string())
        else {
            let unknown = format!("the metadata group holds no principal {principal}");
            drop(reply.send(Err(ErrorReply::new(ErrorCode::Refused, unknown))));
            return Ok(());
        };
        // The setting is at most 2^20, so the cast is lossless.
        let full = reads.sessions() >= self.metadata.config().sessions_max() as usize;
        if full && self.marks.is_quiet(now_us) {
            let quiet = "the registry is full, and the group evicts no session for twice transaction_ms after it begins to listen";
            drop(reply.send(Err(ErrorReply::new(ErrorCode::SessionBusy, quiet))));
            return Ok(());
        }
        let marks = &self.marks;
        match self
            .metadata
            .register_session(session, &name, |held| marks.protects(held))
        {
            Ok(_) => self.durable.push(Durable::Registered(reply)),
            // Every other session is marked, or the evictions awaiting a close are at their bound: either passes, so the frontend
            // answers its client's Register with 14, and the client registers again after a delay (`PROTOCOL.md` §11.5).
            Err(refused) if matches!(refused.0, ClusterErrorEnum::Full { .. }) => {
                let busy = ErrorReply::new(ErrorCode::SessionBusy, refused.to_string());
                drop(reply.send(Err(busy)));
            }
            Err(refused) => drop(reply.send(Err(refused_or_stop(&refused)?))),
        }
        Ok(())
    }
}
