/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::PartitionError;
use ultragraphdb_common_storage::{error_reply, registration_outcome, scan_refusal};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_protocol_wire::{ConflictKind, Direction, ErrorCode, RequestTerm};
use ultragraphdb_store_apply::ChangesetError;
use ultragraphdb_store_posting::PostingError;
use ultragraphdb_store_quads::QuadError;
use ultragraphdb_transactions_commit::{CommitError, PipelineError};
use ultragraphdb_transactions_txn::TxnError;

fn said(error: &PartitionError) -> (ErrorCode, u64) {
    let reply = error_reply(error);
    (reply.code(), reply.detail())
}

/// A failed compare-and-set says what it found in the frame's value (`PROTOCOL.md` §9.4), by kind of term, and nothing when
/// the predicate held none.
#[test]
fn test_a_failed_compare_and_set_carries_the_value_found() {
    let found = |text: Option<&str>| {
        let refused = CommitError::operation_failed(text.map(str::to_string));
        error_reply(&refused.into()).value().cloned()
    };
    assert_eq!(
        found(Some("\"done\"")),
        Some(RequestTerm::Literal {
            lexical: "done".to_string(),
            datatype: None,
        })
    );
    let integer = "http://www.w3.org/2001/XMLSchema#integer";
    assert_eq!(
        found(Some(&format!("\"5\"^^<{integer}>"))),
        Some(RequestTerm::Literal {
            lexical: "5".to_string(),
            datatype: Some(integer.to_string()),
        })
    );
    assert_eq!(
        found(Some("\"chat\"@fr")),
        Some(RequestTerm::LanguageLiteral {
            lexical: "chat".to_string(),
            language: "fr".to_string(),
            direction: None,
        })
    );
    assert_eq!(
        found(Some("<http://ex.org/a>")),
        Some(RequestTerm::Iri("http://ex.org/a".to_string()))
    );
    assert_eq!(found(None), None);
}

/// A partition's failed compare-and-set whose value found passes the Error frame's room reaches its peer within the floor of
/// `max_frame`, the value's lexical form cut and the flag set (`PROTOCOL.md` §9.4).
#[test]
fn test_a_value_found_past_the_floor_is_sent_cut_and_flagged() -> ultragraphdb_tool_bdd::TestResult
{
    use ultragraphdb_protocol_wire::{
        BodyBuilder, BodyView, Command, DETAIL_VALUE_CUT, ErrorReplyView, FrameView, Header,
        MAX_FRAME_FLOOR, RequestTermView,
    };
    let lexical = "v".repeat(MAX_FRAME_FLOOR);
    let refused = CommitError::operation_failed(Some(format!("\"{lexical}\"")));
    let reply = error_reply(&refused.into());
    assert!(reply.value_cut());
    let frame = reply.frame(Header::new(1, Command::Error))?;
    let view = ErrorReplyView::from_frame(&FrameView::validate(&frame, 1, MAX_FRAME_FLOOR)?)?;
    assert_eq!(view.code(), ErrorCode::OperationFailed);
    assert_eq!(view.detail(), DETAIL_VALUE_CUT);
    assert!(view.value_cut());
    assert!(matches!(
        view.value(),
        Some(RequestTermView::Literal { lexical: cut, datatype: None })
            if cut.len() < lexical.len() && lexical.starts_with(cut)
    ));
    Ok(())
}

/// A read past the read set's room states the limit in bytes.
#[test]
fn test_a_read_set_too_large_states_its_limit() {
    let refused = TxnError::read_set_too_large(9_000, 1_024);
    assert_eq!(said(&refused.into()), (ErrorCode::TooLarge, 1_024));
}

/// A commit's write set past a round's bound states that bound in bytes.
#[test]
fn test_a_commit_too_large_states_its_limit() {
    let refused = CommitError::too_large(9_000, 1_024);
    assert_eq!(said(&refused.into()), (ErrorCode::TooLarge, 1_024));
}

/// A write past `statements_per_object_max` is code 6, stating the bound.
#[test]
fn test_a_statement_past_its_bound_states_the_bound() {
    let refused = TxnError::changeset(ChangesetError::statements_per_object(16_000));
    assert_eq!(said(&refused.into()), (ErrorCode::TooLarge, 16_000));
}

/// A transaction past its limit is code 4, stating the limit.
#[test]
fn test_a_transaction_past_its_limit_states_it() {
    assert_eq!(
        said(&TxnError::past_limit(10_000).into()),
        (ErrorCode::PastLimit, 10_000)
    );
}

/// A request of a transaction already ended finds a stream that holds none.
#[test]
fn test_an_ended_transaction_is_refused_as_not_held() {
    assert_eq!(said(&TxnError::ended().into()), (ErrorCode::Protocol, 0));
}

/// A read below what the partition retains is code 3, whichever layer refused it.
#[test]
fn test_a_read_below_what_is_retained_is_too_old() {
    let cases: [QuadError; 2] = [
        PostingError::ts_too_old().into(),
        StoreError::ts_too_old().into(),
    ];
    for case in cases {
        assert_eq!(said(&case.into()), (ErrorCode::TooOld, 0));
    }
    let store: PostingError = StoreError::ts_too_old().into();
    assert_eq!(said(&QuadError::from(store).into()), (ErrorCode::TooOld, 0));
    let poisoned: QuadError = StoreError::poisoned().into();
    assert_eq!(said(&poisoned.into()), (ErrorCode::Unavailable, 0));
}

/// A Bind repeating an identity the partition holds is a protocol error.
#[test]
fn test_a_held_identity_is_a_protocol_error() {
    assert_eq!(
        said(&PartitionError::transaction_held([7; 16])),
        (ErrorCode::Protocol, 0)
    );
}

/// What a Recall refuses with: its request's state in the session table, each by its code and detail.
#[test]
fn test_a_recall_refused_says_why() {
    assert_eq!(
        said(&CommitError::session_busy(1).into()),
        (ErrorCode::SessionBusy, 0)
    );
    assert_eq!(
        said(&CommitError::request_out_of_order(9).into()),
        (ErrorCode::RequestOutOfOrder, 9)
    );
    assert_eq!(
        said(&CommitError::not_committed(5).into()),
        (ErrorCode::NotCommitted, 5)
    );
}

/// A Commit reply past the floor of `max_frame` is refused as too large, naming the floor.
#[test]
fn test_a_reply_too_large_names_the_floor() {
    assert_eq!(
        said(&CommitError::reply_too_large(600_000, 524_288).into()),
        (ErrorCode::TooLarge, 524_288)
    );
}

/// A conflict states its kind.
#[test]
fn test_a_conflict_states_its_kind() {
    for kind in ConflictKind::ALL {
        assert_eq!(
            said(&CommitError::conflict(kind).into()),
            (ErrorCode::Conflict, kind.number())
        );
    }
}

/// A store that failed is code 17 through every layer that carries it; a request it refuses as it stands stays code 7.
#[test]
fn test_a_failed_store_is_unavailable_through_every_layer() {
    let failed = StoreError::read_failed(b"k".to_vec(), "a checksum".to_string());
    let cases: [PartitionError; 5] = [
        QuadError::from(failed.clone()).into(),
        QuadError::from(PostingError::from(failed.clone())).into(),
        ChangesetError::from(failed.clone()).into(),
        ChangesetError::from(QuadError::from(failed.clone())).into(),
        TxnError::changeset(ChangesetError::from(failed)).into(),
    ];
    for refusal in &cases {
        assert_eq!(said(refusal), (ErrorCode::Unavailable, 0), "{refusal}");
        assert_eq!(
            registration_outcome(refusal),
            ErrorCode::Unavailable.number()
        );
    }
    let refused: PartitionError =
        QuadError::from(StoreError::key_too_large(9, 1, Vec::new())).into();
    assert_eq!(said(&refused), (ErrorCode::Refused, 0));
    assert_eq!(registration_outcome(&refused), ErrorCode::Refused.number());
}

/// A wrong partition states its bucket in bits 0 to 23 and its owner in bits 32 to 63, and a home's reply says it with outcome 26.
#[test]
fn test_a_list_failing_its_posting_decode_is_unavailable_and_names_its_key() {
    let key = b"list".to_vec();
    for failure in [
        PostingError::unexpected_meta(1, key.clone()),
        PostingError::missing_part(key.clone()),
    ] {
        let refusal = PartitionError::from(QuadError::from(failure));
        assert_eq!(error_reply(&refusal).code(), ErrorCode::Unavailable);
        assert!(
            scan_refusal(b"at", &refusal).message().contains(
                &key.iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            ),
            "{refusal}"
        );
    }
}

#[test]
fn test_a_wrong_partition_states_its_bucket_and_owner() {
    let refusal = PartitionError::wrong_partition(30_000, 2);
    assert_eq!(
        said(&refusal),
        (ErrorCode::WrongPartition, 30_000 | (2 << 32))
    );
    assert_eq!(
        registration_outcome(&refusal),
        ErrorCode::WrongPartition.number()
    );
}

/// A home's outcome ends its registrant's transaction with code 17 for a failed store, with code 26 of `detail` 0 for a bucket the
/// home does not own, and with code 7 otherwise.
#[test]
fn test_a_homes_outcome_becomes_its_code() {
    let failed = PartitionError::refused(ErrorCode::Unavailable.number());
    assert_eq!(said(&failed), (ErrorCode::Unavailable, 0));
    let elsewhere = PartitionError::refused(ErrorCode::WrongPartition.number());
    assert_eq!(said(&elsewhere), (ErrorCode::WrongPartition, 0));
    let refused = PartitionError::refused(ErrorCode::Refused.number());
    assert_eq!(said(&refused), (ErrorCode::Refused, 0));
}

/// A Scan's refusal names the key the store failed under, through every layer, and where the read stood only when the store named
/// none; any other is the partition's own.
#[test]
fn test_a_scans_refusal_names_the_key_of_a_failed_store() {
    let failing = StoreError::read_failed(b"k".to_vec(), "a checksum".to_string());
    let direct: PartitionError = QuadError::from(failing.clone()).into();
    let through_txn: PartitionError =
        TxnError::changeset(ChangesetError::from(QuadError::from(failing))).into();
    for failed in [direct, through_txn] {
        let reply = scan_refusal(&[0xAB, 0x01], &failed);
        assert_eq!(reply.code(), ErrorCode::Unavailable);
        let named = format!("at the key {:02x}:", b'k');
        assert!(reply.message().contains(&named), "{}", reply.message());
    }
    let unnamed: PartitionError = QuadError::from(StoreError::read_failed(
        Vec::new(),
        "a checksum".to_string(),
    ))
    .into();
    let reply = scan_refusal(&[0xAB, 0x01], &unnamed);
    assert!(
        reply.message().contains("at the key ab01:"),
        "{}",
        reply.message()
    );
    let other = TxnError::past_limit(10_000).into();
    assert_eq!(scan_refusal(&[0xAB], &other), error_reply(&other));
}

/// Cross-partition work states the transaction's partition in bits 0 to 31 and the other partition in bits 32 to 63
/// (`PROTOCOL.md` §10, code 23), whichever layer refused it.
#[test]
fn test_a_cross_partition_refusal_states_both_partitions() {
    let (partition, other) = (3_u32, 5_u32);
    let detail = u64::from(partition) + u64::from(other) * (1 << 32);
    let cases: [PartitionError; 3] = [
        PartitionError::cross_partition(partition, other),
        TxnError::cross_partition(partition, other).into(),
        CommitError::cross_partition(partition, other).into(),
    ];
    for refusal in &cases {
        assert_eq!(
            said(refusal),
            (ErrorCode::CrossPartition, detail),
            "{refusal}"
        );
    }
}

/// Each refusal's code and detail as `PROTOCOL.md` §13 states them, for the kinds the other tests leave out.
#[test]
fn test_every_other_refusal_takes_its_code() {
    let stored = || StoreError::read_failed(b"k".to_vec(), "a checksum".to_string());
    let cases: Vec<(PartitionError, (ErrorCode, u64))> = vec![
        (PartitionError::timed_out(4), (ErrorCode::TimedOut, 4)),
        (
            PipelineError::sessions(stored()).into(),
            (ErrorCode::Unavailable, 0),
        ),
        (
            TxnError::read_above_watermark(9, 7).into(),
            (ErrorCode::AheadOfWatermark, 7),
        ),
        (
            TxnError::write_set_too_large(9_000, 1_024).into(),
            (ErrorCode::TooLarge, 1_024),
        ),
        (TxnError::read_only().into(), (ErrorCode::Refused, 0)),
        (TxnError::tenant(1, 2).into(), (ErrorCode::Refused, 0)),
        (TxnError::declaration().into(), (ErrorCode::Refused, 0)),
        (
            ChangesetError::from(PostingError::ts_too_old()).into(),
            (ErrorCode::TooOld, 0),
        ),
        (
            ChangesetError::from(PostingError::not_a_list(b"k")).into(),
            (ErrorCode::Refused, 0),
        ),
        (
            ChangesetError::rebuilding("http://ex.org/p").into(),
            (ErrorCode::Refused, 0),
        ),
        (
            QuadError::corrupted(b"k", "a key").into(),
            (ErrorCode::Unavailable, 0),
        ),
        (
            QuadError::from(PostingError::invalid_key()).into(),
            (ErrorCode::Unavailable, 0),
        ),
        (QuadError::pending_full(8).into(), (ErrorCode::Refused, 0)),
        (CommitError::too_old(3, 5).into(), (ErrorCode::TooOld, 0)),
        (CommitError::busy(8).into(), (ErrorCode::Busy, 0)),
        (
            CommitError::refused("no".to_string()).into(),
            (ErrorCode::Refused, 0),
        ),
        (CommitError::unsealed().into(), (ErrorCode::Unavailable, 0)),
    ];
    for (refusal, want) in &cases {
        assert_eq!(said(refusal), *want, "{refusal}");
    }
}

/// A changeset's failed compare-and-set says what it found as a commit's does, a blank node by its label and a literal
/// with its base direction; a triple term found is no value a request names.
#[test]
fn test_a_changeset_s_failed_compare_and_set_carries_the_value_found() {
    let found = |text: &str| {
        let refused = ChangesetError::operation_failed(Some(text.to_string()));
        error_reply(&refused.into()).value().cloned()
    };
    assert_eq!(
        found("_:b1"),
        Some(RequestTerm::BlankNode("b1".to_string()))
    );
    for (text, direction) in [
        ("\"chat\"@fr--ltr", Direction::Ltr),
        ("\"chat\"@ar--rtl", Direction::Rtl),
    ] {
        assert!(
            matches!(
                found(text),
                Some(RequestTerm::LanguageLiteral { direction: Some(said), .. }) if said == direction
            ),
            "{text}"
        );
    }
    assert_eq!(
        found("<<( <http://ex.org/a> <http://ex.org/p> <http://ex.org/b> )>>"),
        None
    );
}

/// A Scan's refusal names the key a failure names through a changeset and a list, a corrupted quad's key, and where the
/// read stood for a failure that names none.
#[test]
fn test_a_scans_refusal_names_the_key_through_every_carrier() {
    let key = b"k".to_vec();
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let at = [0xAB_u8, 0x01];
    let failed = || StoreError::read_failed(key.clone(), "a checksum".to_string());
    let named: Vec<PartitionError> = vec![
        ChangesetError::from(failed()).into(),
        ChangesetError::from(PostingError::store(failed())).into(),
        ChangesetError::from(PostingError::schema_posting(key.clone())).into(),
        ChangesetError::from(QuadError::from(failed())).into(),
        QuadError::corrupted(&key, "a key").into(),
        TxnError::changeset(ChangesetError::from(PostingError::missing_part(
            key.clone(),
        )))
        .into(),
    ];
    for refusal in &named {
        let reply = scan_refusal(&at, refusal);
        assert!(
            reply
                .message()
                .contains(&format!("at the key {}:", hex(&key))),
            "{refusal}: {}",
            reply.message()
        );
    }
    let unnamed: Vec<PartitionError> = vec![
        ChangesetError::from(PostingError::invalid_key()).into(),
        ChangesetError::from(StoreError::poisoned()).into(),
        QuadError::from(StoreError::write_failed("a disk".to_string())).into(),
        PipelineError::sessions(failed()).into(),
    ];
    for refusal in &unnamed {
        let reply = scan_refusal(&at, refusal);
        assert!(
            reply
                .message()
                .contains(&format!("at the key {}:", hex(&at))),
            "{refusal}: {}",
            reply.message()
        );
    }
}
