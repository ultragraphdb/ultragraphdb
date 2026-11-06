/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::str::FromStr;

use oxrdf::{BaseDirection, Term};
use ultragraphdb_cluster_partition::{PartitionError, PartitionErrorEnum};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_protocol_wire::{Direction, ErrorCode, ErrorReply, RequestTerm};
use ultragraphdb_store_apply::{ChangesetError, ChangesetErrorEnum};
use ultragraphdb_store_posting::{PostingError, PostingErrorEnum};
use ultragraphdb_store_quads::{QuadError, QuadErrorEnum};
use ultragraphdb_transactions_commit::{CommitError, CommitErrorEnum};
use ultragraphdb_transactions_txn::{TxnError, TxnErrorEnum};

/// What a refusal of the partition says on the wire (`PROTOCOL.md` §10), its message the refusal's own.
#[must_use]
pub fn error_reply(error: &PartitionError) -> ErrorReply {
    let message = error.to_string();
    let (code, detail) = match &error.0 {
        PartitionErrorEnum::WrongPartition { bucket, owner } => (
            ErrorCode::WrongPartition,
            u64::from(*bucket) | (u64::from(*owner) << 32),
        ),
        PartitionErrorEnum::CrossPartition { partition, other } => (
            ErrorCode::CrossPartition,
            u64::from(*partition) | (u64::from(*other) << 32),
        ),
        PartitionErrorEnum::TimedOut { partition } => (ErrorCode::TimedOut, u64::from(*partition)),
        PartitionErrorEnum::Refused { outcome } => outcome_code(*outcome),
        PartitionErrorEnum::TransactionHeld { .. } => (ErrorCode::Protocol, 0),
        PartitionErrorEnum::Changeset(refusal) => changeset_code(refusal),
        PartitionErrorEnum::Quad(refusal) => quad_code(refusal),
        PartitionErrorEnum::Pipeline(_) => (ErrorCode::Unavailable, 0),
        PartitionErrorEnum::Txn(refusal) => txn_code(refusal),
        PartitionErrorEnum::Commit(refusal) => commit_code(refusal),
    };
    let reply = ErrorReply::new(code, message).with_detail(detail);
    match found_value(error) {
        Some(value) => reply.with_value(value),
        None => reply,
    }
}

/// The value a failed compare-and-set found, as the Error frame's sections 1 and 2 hold it (`PROTOCOL.md` §9.4): the commit
/// step and the store state it as an N-Triples term. `None` when the predicate held none, or for any other refusal.
fn found_value(error: &PartitionError) -> Option<RequestTerm> {
    let found = match &error.0 {
        PartitionErrorEnum::Commit(refusal) => refusal.found(),
        PartitionErrorEnum::Changeset(ChangesetError(ChangesetErrorEnum::OperationFailed {
            found,
        })) => found.as_deref(),
        PartitionErrorEnum::WrongPartition { .. }
        | PartitionErrorEnum::CrossPartition { .. }
        | PartitionErrorEnum::TimedOut { .. }
        | PartitionErrorEnum::Refused { .. }
        | PartitionErrorEnum::TransactionHeld { .. }
        | PartitionErrorEnum::Changeset(_)
        | PartitionErrorEnum::Quad(_)
        | PartitionErrorEnum::Pipeline(_)
        | PartitionErrorEnum::Txn(_) => None,
    }?;
    match Term::from_str(found).ok()? {
        Term::NamedNode(node) => Some(RequestTerm::Iri(node.into_string())),
        Term::BlankNode(node) => Some(RequestTerm::BlankNode(node.into_string())),
        Term::Literal(literal) => Some(literal_term(&literal)),
        Term::Triple(_) => None,
    }
}

/// A literal as a request names it: a plain string with no datatype's IRI, a language-tagged one by its tag.
fn literal_term(literal: &oxrdf::Literal) -> RequestTerm {
    let lexical = literal.value().to_string();
    if let Some(language) = literal.language() {
        return RequestTerm::LanguageLiteral {
            lexical,
            language: language.to_string(),
            direction: literal.direction().map(|direction| match direction {
                BaseDirection::Ltr => Direction::Ltr,
                BaseDirection::Rtl => Direction::Rtl,
            }),
        };
    }
    let datatype = literal.datatype();
    RequestTerm::Literal {
        lexical,
        datatype: (datatype != oxrdf::vocab::xsd::STRING).then(|| datatype.as_str().to_string()),
    }
}

fn txn_code(error: &TxnError) -> (ErrorCode, u64) {
    match &error.0 {
        TxnErrorEnum::ReadAboveWatermark { watermark, .. } => {
            (ErrorCode::AheadOfWatermark, *watermark)
        }
        TxnErrorEnum::CrossPartition { partition, other } => (
            ErrorCode::CrossPartition,
            u64::from(*partition) | (u64::from(*other) << 32),
        ),
        TxnErrorEnum::PastLimit { limit_ms } => (ErrorCode::PastLimit, *limit_ms),
        TxnErrorEnum::Ended => (ErrorCode::Protocol, 0),
        TxnErrorEnum::WriteSetTooLarge { limit, .. }
        | TxnErrorEnum::ReadSetTooLarge { limit, .. } => (ErrorCode::TooLarge, *limit as u64),
        TxnErrorEnum::Changeset(refusal) => changeset_code(refusal),
        TxnErrorEnum::ReadOnly | TxnErrorEnum::Tenant { .. } | TxnErrorEnum::Declaration => {
            (ErrorCode::Refused, 0)
        }
    }
}

/// What a home's outcome ends its registrant's transaction with: code 17 for a store that failed at the home, so no failed store
/// reads as a skolem IRI nobody holds; code 26 for a bucket the home does not own, `detail` 0 since the registrant does not know
/// the owner; and otherwise code 7 (`PROTOCOL.md` §13.15).
fn outcome_code(outcome: u16) -> (ErrorCode, u64) {
    if outcome == ErrorCode::Unavailable.number() {
        (ErrorCode::Unavailable, 0)
    } else if outcome == ErrorCode::WrongPartition.number() {
        (ErrorCode::WrongPartition, 0)
    } else {
        (ErrorCode::Refused, 0)
    }
}

/// A read below what the partition retains is code 3 (`PROTOCOL.md` §11.9); a write past `statements_per_object_max`, code 6
/// with its bound; a store that failed, code 17; any other refusal of a changeset, code 7.
fn changeset_code(error: &ChangesetError) -> (ErrorCode, u64) {
    if let ChangesetErrorEnum::Quad(quad) = &error.0 {
        return quad_code(quad);
    }
    if let ChangesetErrorEnum::StatementsPerObject { limit } = &error.0 {
        return (ErrorCode::TooLarge, *limit as u64);
    }
    if let ChangesetErrorEnum::Posting(posting) = &error.0 {
        return posting_code(posting);
    }
    if let ChangesetErrorEnum::Store(store) = &error.0 {
        return store_code(store);
    }
    (ErrorCode::Refused, 0)
}

fn quad_code(error: &QuadError) -> (ErrorCode, u64) {
    if let QuadErrorEnum::Posting(posting) = &error.0 {
        return posting_code(posting);
    }
    if let QuadErrorEnum::Store(store) = &error.0 {
        return store_code(store);
    }
    if matches!(&error.0, QuadErrorEnum::Corrupted { .. }) {
        return (ErrorCode::Unavailable, 0);
    }
    (ErrorCode::Refused, 0)
}

/// Code 3 for a read below what the list answers; code 17 for a store that failed or a list that fails its decode, which reads
/// as corruption and never as a refused request (`PROTOCOL.md` §13.13); otherwise code 7.
fn posting_code(error: &PostingError) -> (ErrorCode, u64) {
    match &error.0 {
        PostingErrorEnum::Store(store) => store_code(store),
        PostingErrorEnum::TsTooOld | PostingErrorEnum::ReadBelowMinTs { .. } => {
            (ErrorCode::TooOld, 0)
        }
        PostingErrorEnum::InvalidKey
        | PostingErrorEnum::SchemaPosting { .. }
        | PostingErrorEnum::UnexpectedMeta { .. }
        | PostingErrorEnum::MissingPart { .. } => (ErrorCode::Unavailable, 0),
        PostingErrorEnum::NotAList { .. }
        | PostingErrorEnum::DeleteBelowMinTs { .. }
        | PostingErrorEnum::HighPriorityOp
        | PostingErrorEnum::StaleVersion { .. } => (ErrorCode::Refused, 0),
    }
}

/// Code 3 for a read below the discard version, code 17 for a store that failed under the request (`PROTOCOL.md` §§9.4, 13.13),
/// and code 7 for a request the store refused as it stands.
fn store_code(error: &StoreError) -> (ErrorCode, u64) {
    match &error.0 {
        StoreErrorEnum::TsTooOld => (ErrorCode::TooOld, 0),
        StoreErrorEnum::ReadFailed { .. }
        | StoreErrorEnum::Corrupted { .. }
        | StoreErrorEnum::WriteFailed(_)
        | StoreErrorEnum::Poisoned
        | StoreErrorEnum::OpenFailed(_) => (ErrorCode::Unavailable, 0),
        StoreErrorEnum::KeyTooLarge { .. }
        | StoreErrorEnum::Unsupported(_)
        | StoreErrorEnum::UnknownTemporary(_)
        | StoreErrorEnum::EmptyKey => (ErrorCode::Refused, 0),
    }
}

fn commit_code(error: &CommitError) -> (ErrorCode, u64) {
    match &error.0 {
        CommitErrorEnum::Conflict { kind } => (ErrorCode::Conflict, kind.number()),
        CommitErrorEnum::TooOld { .. } => (ErrorCode::TooOld, 0),
        CommitErrorEnum::TooLarge { limit, .. } | CommitErrorEnum::ReplyTooLarge { limit, .. } => {
            (ErrorCode::TooLarge, *limit as u64)
        }
        CommitErrorEnum::Busy { .. } => (ErrorCode::Busy, 0),
        CommitErrorEnum::RequestOutOfOrder { last, .. } => (ErrorCode::RequestOutOfOrder, *last),
        CommitErrorEnum::SessionBusy { .. } => (ErrorCode::SessionBusy, 0),
        CommitErrorEnum::NotCommitted { request } => (ErrorCode::NotCommitted, *request),
        CommitErrorEnum::OperationFailed { .. } => (ErrorCode::OperationFailed, 0),
        CommitErrorEnum::Refused { .. } => (ErrorCode::Refused, 0),
        CommitErrorEnum::CrossPartition { partition, other } => (
            ErrorCode::CrossPartition,
            u64::from(*partition) | (u64::from(*other) << 32),
        ),
        CommitErrorEnum::Malformed { .. }
        | CommitErrorEnum::Unsealed
        | CommitErrorEnum::Unavailable
        | CommitErrorEnum::CloseOutOfOrder { .. } => (ErrorCode::Unavailable, 0),
    }
}

/// What a Scan's stream is told of `refusal`, met reading on from `at`: a store failed under the range names the key that failed, or
/// `at` when the store named none, so a failed read never reads as absence (`PROTOCOL.md` §13.13); any other refusal as
/// [`error_reply`] says it.
#[must_use]
pub fn scan_refusal(at: &[u8], refusal: &PartitionError) -> ErrorReply {
    let reply = error_reply(refusal);
    if reply.code() != ErrorCode::Unavailable {
        return reply;
    }
    let key = failed_key(refusal).unwrap_or(at);
    let hex: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
    ErrorReply::new(
        ErrorCode::Unavailable,
        format!("a Scan's read failed at the key {hex}: {}", reply.message()),
    )
}

/// The key a failed read of `refusal` names, through every layer it passed.
fn failed_key(refusal: &PartitionError) -> Option<&[u8]> {
    if let PartitionErrorEnum::Quad(quad) = &refusal.0 {
        return quad_key(quad);
    }
    if let PartitionErrorEnum::Changeset(changeset) = &refusal.0 {
        return changeset_key(changeset);
    }
    if let PartitionErrorEnum::Txn(TxnError(TxnErrorEnum::Changeset(changeset))) = &refusal.0 {
        return changeset_key(changeset);
    }
    None
}

fn changeset_key(error: &ChangesetError) -> Option<&[u8]> {
    if let ChangesetErrorEnum::Quad(quad) = &error.0 {
        return quad_key(quad);
    }
    if let ChangesetErrorEnum::Posting(posting) = &error.0 {
        return posting_key(posting);
    }
    if let ChangesetErrorEnum::Store(store) = &error.0 {
        return store_key(store);
    }
    None
}

fn quad_key(error: &QuadError) -> Option<&[u8]> {
    if let QuadErrorEnum::Posting(posting) = &error.0 {
        return posting_key(posting);
    }
    if let QuadErrorEnum::Store(store) = &error.0 {
        return store_key(store);
    }
    if let QuadErrorEnum::Corrupted { key, .. } = &error.0 {
        return Some(key);
    }
    None
}

fn posting_key(error: &PostingError) -> Option<&[u8]> {
    match &error.0 {
        PostingErrorEnum::Store(store) => store_key(store),
        PostingErrorEnum::SchemaPosting { key }
        | PostingErrorEnum::UnexpectedMeta { key, .. }
        | PostingErrorEnum::MissingPart { key } => Some(key),
        PostingErrorEnum::NotAList { .. }
        | PostingErrorEnum::InvalidKey
        | PostingErrorEnum::TsTooOld
        | PostingErrorEnum::ReadBelowMinTs { .. }
        | PostingErrorEnum::DeleteBelowMinTs { .. }
        | PostingErrorEnum::HighPriorityOp
        | PostingErrorEnum::StaleVersion { .. } => None,
    }
}

/// The key `error` names, when it names one.
fn store_key(error: &StoreError) -> Option<&[u8]> {
    if let StoreErrorEnum::ReadFailed { key, .. } | StoreErrorEnum::Corrupted { key, .. } = &error.0
    {
        return (!key.is_empty()).then_some(key.as_slice());
    }
    None
}

/// The outcome a home's reply says for a registration it refused with `refusal` (`PROTOCOL.md` §13.15): 26 for a bucket it does
/// not own, 17 for a store that failed under it, and otherwise 7.
#[must_use]
pub fn registration_outcome(refusal: &PartitionError) -> u16 {
    let code = error_reply(refusal).code();
    if code == ErrorCode::WrongPartition || code == ErrorCode::Unavailable {
        code.number()
    } else {
        ErrorCode::Refused.number()
    }
}
