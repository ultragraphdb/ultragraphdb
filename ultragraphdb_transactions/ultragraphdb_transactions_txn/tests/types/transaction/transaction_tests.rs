/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use oxrdf::{Literal, Term};
use ultragraphdb_core_mapping::{Change, Changeset, Declaration, PredicateDeclaration};
use ultragraphdb_protocol_wire::WriteSetRecordView;
use ultragraphdb_store_apply::{ChangesetError, ChangesetErrorEnum};
use ultragraphdb_store_sim::Fault;
use ultragraphdb_transactions_txn::{
    Commit, Mode, Prepared, Transaction, TxnBuffers, TxnError, TxnErrorEnum, TxnLimits,
};

use crate::utils::fixture::{
    NS, SECOND, applier, begin, begin_with, changeset, commit, ex, held, insert, insert_literal,
    objects,
};

fn literal(value: &str) -> Term {
    Literal::new_simple_literal(value).into()
}

fn prepared(commit: Result<Commit, TxnError>) -> Prepared {
    match commit.expect("the commit") {
        Commit::ReadWrite(prepared) => prepared,
        Commit::ReadOnly(_) => panic!("a read-write transaction ends on its worker"),
    }
}

#[test]
fn test_a_transaction_begins_at_the_watermark() {
    let applier = applier();
    let watermark = commit(&applier, vec![insert_literal("a", "name", "A")]);
    let txn = begin(&applier, Mode::ReadWrite);
    assert_eq!(txn.read_version(), watermark);
    assert_eq!(txn.namespace(), NS);
    assert_eq!(txn.mode(), Mode::ReadWrite);
    assert!(!txn.is_ended());
}

#[test]
fn test_a_named_version_at_or_below_the_watermark_is_read() {
    let applier = applier();
    let first = commit(&applier, vec![insert_literal("a", "name", "A")]);
    let second = commit(&applier, vec![insert_literal("a", "name", "B")]);
    assert!(first < second);
    for (version, expected) in [
        (first, vec![literal("A")]),
        (second, vec![literal("A"), literal("B")]),
    ] {
        let mut txn = Transaction::begin(
            &applier,
            NS,
            Mode::ReadOnly,
            version,
            0,
            TxnLimits::default(),
            TxnBuffers::new(),
        )
        .expect("at or below the watermark");
        assert_eq!(txn.read_version(), version);
        let mut read = objects(&mut txn, "a", "name", 0).unwrap();
        read.sort_by_key(|t| t.to_string());
        assert_eq!(read, expected);
    }
}

#[test]
fn test_a_version_above_the_watermark_is_refused_at_once_naming_both() {
    let applier = applier();
    let watermark = commit(&applier, vec![insert_literal("a", "name", "A")]);
    for above in [watermark + 1, watermark + 2, u64::MAX] {
        let refused = Transaction::begin(
            &applier,
            NS,
            Mode::ReadWrite,
            above,
            0,
            TxnLimits::default(),
            TxnBuffers::new(),
        );
        assert_eq!(
            refused.err(),
            Some(TxnError::ReadAboveWatermark(above, watermark))
        );
    }
    // The worker goes on: the next transaction begins and reads.
    let mut next = begin(&applier, Mode::ReadOnly);
    assert_eq!(
        objects(&mut next, "a", "name", 0).unwrap(),
        vec![literal("A")]
    );
}

#[test]
fn test_a_commit_after_the_transaction_began_is_not_visible_to_it() {
    let applier = applier();
    commit(&applier, vec![insert_literal("a", "name", "A")]);
    let mut txn = begin(&applier, Mode::ReadWrite);
    commit(&applier, vec![insert_literal("b", "name", "B")]);
    // `b` was never read before the commit, so the snapshot, not a cache, keeps it out.
    assert!(objects(&mut txn, "b", "name", 0).unwrap().is_empty());
    assert_eq!(held(&applier, "b", "name"), vec![literal("B")]);
}

#[test]
fn test_a_read_sees_the_transactions_own_writes_across_requests_and_no_one_elses() {
    let applier = applier();
    commit(&applier, vec![insert("a", "knows", "z")]);
    let mut txn = begin(&applier, Mode::ReadWrite);
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .unwrap();
    txn.write(&changeset(vec![insert("a", "knows", "c")]), 0)
        .unwrap();
    let mut read: Vec<String> = objects(&mut txn, "a", "knows", 0)
        .unwrap()
        .iter()
        .map(Term::to_string)
        .collect();
    read.sort();
    let expected: Vec<String> = ["b", "c", "z"]
        .iter()
        .map(|n| Term::from(ex(n)).to_string())
        .collect();
    assert_eq!(read, expected);
    // A delete in a later request is seen too.
    let delete = Change::Delete {
        graph: oxrdf::GraphName::DefaultGraph,
        subject: ex("a").into(),
        predicate: ex("knows"),
        object: ex("z").into(),
    };
    txn.write(&changeset(vec![delete]), 0).unwrap();
    assert_eq!(objects(&mut txn, "a", "knows", 0).unwrap().len(), 2);
    // Nobody else sees them.
    assert_eq!(held(&applier, "a", "knows"), vec![Term::from(ex("z"))]);
}

#[test]
fn test_a_subject_the_transaction_created_reads_back_with_its_new_terms() {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadWrite);
    txn.write(
        &changeset(vec![
            insert("new", "knows", "other"),
            insert_literal("new", "name", "fresh"),
        ]),
        0,
    )
    .unwrap();
    assert_eq!(
        objects(&mut txn, "new", "knows", 0).unwrap(),
        vec![Term::from(ex("other"))]
    );
    assert_eq!(
        objects(&mut txn, "new", "name", 0).unwrap(),
        vec![literal("fresh")]
    );
}

#[test]
fn test_a_read_of_terms_the_store_lacks_holds_nothing_and_records_them_absent() {
    let applier = applier();
    commit(&applier, vec![insert("a", "knows", "b")]);
    let mut txn = begin(&applier, Mode::ReadWrite);
    assert!(objects(&mut txn, "nobody", "knows", 0).unwrap().is_empty());
    assert!(objects(&mut txn, "a", "unknown", 0).unwrap().is_empty());
    let prepared = prepared(txn.commit(1, 1, &[], 0));
    let absent: Vec<&[u8]> = prepared.read_set().absent().collect();
    assert!(
        absent.contains(&ultragraphdb_store_quads::iri_key(NS, ex("nobody").as_str()).as_slice())
    );
    assert!(
        absent.contains(&ultragraphdb_store_quads::iri_key(NS, ex("unknown").as_str()).as_slice())
    );
}

#[test]
fn test_a_read_write_transactions_reads_are_in_its_read_set() {
    let applier = applier();
    commit(&applier, vec![insert("a", "knows", "b")]);
    let mut txn = begin(&applier, Mode::ReadWrite);
    objects(&mut txn, "a", "knows", 0).unwrap();
    txn.write(&changeset(vec![insert("c", "knows", "b")]), 0)
        .unwrap();
    let watermark = txn.read_version();
    let prepared = prepared(txn.commit(5, 6, b"cr", 0));
    let set = prepared.read_set();
    assert!(set.is_sealed());
    assert_eq!((set.namespace(), set.read_version()), (NS, watermark));
    let a = applier
        .quads()
        .uid_of(NS, ex("a").as_str())
        .unwrap()
        .unwrap();
    let knows = applier
        .quads()
        .uid_of(NS, ex("knows").as_str())
        .unwrap()
        .unwrap();
    let forward = ultragraphdb_core_keys::QuadKey::Forward {
        namespace: NS,
        graph: 0,
        predicate: knows,
        subject: a,
    }
    .to_bytes();
    assert!(
        set.lists().any(|k| k == forward.as_slice()),
        "the list read whole"
    );
    let mut catalog = vec![0x01];
    catalog.extend_from_slice(&NS.to_be_bytes());
    assert_eq!(set.catalog().0, catalog.as_slice());
    // The record carries the transaction's identity and writes.
    assert_eq!(prepared.namespace(), NS);
    assert_eq!(prepared.read_version(), watermark);
    let view = WriteSetRecordView::new(prepared.record()).unwrap();
    assert_eq!((view.namespace(), view.read_version()), (NS, watermark));
    assert_eq!((view.session(), view.request()), (5, 6));
    assert_eq!(view.commit_record(), b"cr");
    assert!(!view.list_deltas().is_empty());
    assert_eq!(prepared.executed().read_ts(), watermark);
}

#[test]
fn test_a_read_write_transaction_with_no_writes_is_still_validated() {
    let applier = applier();
    commit(&applier, vec![insert("a", "knows", "b")]);
    let mut txn = begin(&applier, Mode::ReadWrite);
    objects(&mut txn, "a", "knows", 0).unwrap();
    let prepared = prepared(txn.commit(1, 1, &[], 0));
    assert_eq!(prepared.read_set().lists().count(), 1);
    let view = WriteSetRecordView::new(prepared.record()).unwrap();
    assert_eq!(view.list_deltas().len(), 0);
}

#[test]
fn test_a_read_only_transaction_ends_on_its_worker_and_refuses_writes() {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadOnly);
    assert_eq!(
        txn.write(&changeset(vec![insert("a", "knows", "b")]), 0),
        Err(TxnError::ReadOnly())
    );
    assert!(!txn.is_ended(), "a refused write leaves it as it was");
    assert!(objects(&mut txn, "a", "knows", 0).unwrap().is_empty());
    assert!(matches!(txn.commit(0, 0, &[], 0), Ok(Commit::ReadOnly(_))));
    assert!(held(&applier, "a", "knows").is_empty());
}

#[test]
fn test_a_changeset_of_another_namespace_is_refused_and_leaves_the_transaction_as_it_was() {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadWrite);
    let other = Changeset::new(3).with(insert("a", "knows", "b"));
    assert_eq!(txn.write(&other, 0), Err(TxnError::Namespace(NS, 3)));
    assert!(!txn.is_ended());
    txn.write(&changeset(vec![insert("a", "knows", "c")]), 0)
        .unwrap();
    assert_eq!(
        objects(&mut txn, "a", "knows", 0).unwrap(),
        vec![Term::from(ex("c"))]
    );
}

#[test]
fn test_a_refused_changeset_ends_the_transaction() {
    let applier = applier();
    // `age` declared an integer: a string object is refused by the catalog.
    let declared =
        PredicateDeclaration::new(ex("age")).with_datatype(oxrdf::vocab::xsd::INTEGER.into_owned());
    commit(
        &applier,
        vec![Change::Declare(Declaration::Predicate(declared))],
    );
    let mut txn = begin(&applier, Mode::ReadWrite);
    let refused = txn.write(&changeset(vec![insert_literal("a", "age", "old")]), 0);
    assert!(
        matches!(
            refused,
            Err(TxnError(TxnErrorEnum::Changeset(ChangesetError(
                ChangesetErrorEnum::Catalog(_)
            ))))
        ),
        "the catalog's refusal: {refused:?}"
    );
    assert!(!refused.unwrap_err().is_retryable());
    assert!(txn.is_ended());
    assert_eq!(
        txn.write(&changeset(vec![insert("a", "knows", "b")]), 0),
        Err(TxnError::Ended())
    );
    assert_eq!(objects(&mut txn, "a", "knows", 0), Err(TxnError::Ended()));
    assert!(matches!(txn.commit(1, 1, &[], 0), Err(e) if e == TxnError::Ended()));
}

#[test]
fn test_a_read_write_transaction_lives_exactly_its_limit() {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadWrite);
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 10 * SECOND)
        .unwrap();
    objects(&mut txn, "a", "knows", 10 * SECOND).unwrap();
    let prepared = prepared(txn.commit(1, 1, &[], 10 * SECOND));
    // The positive control of "nothing it wrote is held" below: a commit within the limit hands the commit step an
    // execution that, applied, is held.
    let (executed, _) = prepared.into_parts();
    let version = applier.quads().lists().next_version();
    applier.apply_round(vec![(version, executed)]).unwrap();
    assert_eq!(held(&applier, "a", "knows"), vec![Term::from(ex("b"))]);
}

#[test]
fn test_one_microsecond_past_the_limit_refuses_each_request_and_ends_the_transaction() {
    let applier = applier();
    let past = 10 * SECOND + 1;
    let mut txn = begin(&applier, Mode::ReadWrite);
    assert_eq!(
        txn.write(&changeset(vec![insert("a", "knows", "b")]), past),
        Err(TxnError::PastLimit(10_000))
    );
    assert!(txn.is_ended());
    assert_eq!(
        objects(&mut txn, "a", "knows", past),
        Err(TxnError::Ended())
    );

    let mut txn = begin(&applier, Mode::ReadWrite);
    assert_eq!(
        objects(&mut txn, "a", "knows", past),
        Err(TxnError::PastLimit(10_000))
    );
    assert!(txn.is_ended());

    let mut txn = begin(&applier, Mode::ReadWrite);
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .unwrap();
    let error = txn.commit(1, 1, &[], past).unwrap_err();
    assert_eq!(error, TxnError::PastLimit(10_000));
    assert!(error.is_retryable());
    assert!(held(&applier, "a", "knows").is_empty());
}

#[test]
fn test_a_read_only_transaction_lives_sixteen_seconds() {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadOnly);
    objects(&mut txn, "a", "knows", 16 * SECOND).unwrap();
    assert!(matches!(
        txn.commit(0, 0, &[], 16 * SECOND),
        Ok(Commit::ReadOnly(_))
    ));
    let mut txn = begin(&applier, Mode::ReadOnly);
    objects(&mut txn, "a", "knows", 11 * SECOND).unwrap();
    assert_eq!(
        txn.commit(0, 0, &[], 16 * SECOND + 1).unwrap_err(),
        TxnError::PastLimit(16_000)
    );
}

#[test]
fn test_the_limits_are_settings_and_count_from_the_transactions_begin() {
    let applier = applier();
    let limits = TxnLimits::default()
        .with_read_write(Duration::from_millis(1_500))
        .with_read_only(Duration::from_millis(250));
    let began = 5 * SECOND;
    let mut txn = Transaction::begin(
        &applier,
        NS,
        Mode::ReadWrite,
        0,
        began,
        limits,
        TxnBuffers::new(),
    )
    .unwrap();
    objects(&mut txn, "a", "knows", began + 1_500_000).unwrap();
    assert_eq!(
        objects(&mut txn, "a", "knows", began + 1_500_001),
        Err(TxnError::PastLimit(1_500))
    );
    let mut txn = Transaction::begin(
        &applier,
        NS,
        Mode::ReadOnly,
        0,
        began,
        limits,
        TxnBuffers::new(),
    )
    .unwrap();
    assert_eq!(
        objects(&mut txn, "a", "knows", began + 250_001),
        Err(TxnError::PastLimit(250))
    );
}

#[test]
fn test_a_clock_behind_the_begin_is_no_time_passed() {
    let applier = applier();
    let mut txn = Transaction::begin(
        &applier,
        NS,
        Mode::ReadWrite,
        0,
        20 * SECOND,
        TxnLimits::default(),
        TxnBuffers::new(),
    )
    .unwrap();
    objects(&mut txn, "a", "knows", SECOND).unwrap();
    assert!(!txn.is_ended());
}

#[test]
fn test_an_aborted_transaction_writes_nothing_and_returns_its_buffers() {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadWrite);
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .unwrap();
    let buffers = txn.abort();
    assert!(held(&applier, "a", "knows").is_empty());
    let mut again = Transaction::begin(
        &applier,
        NS,
        Mode::ReadWrite,
        0,
        0,
        TxnLimits::default(),
        buffers,
    )
    .unwrap();
    assert!(objects(&mut again, "a", "knows", 0).unwrap().is_empty());
}

#[test]
fn test_a_read_set_past_its_limit_refuses_the_commit() {
    let applier = applier();
    commit(&applier, vec![insert("a", "knows", "b")]);
    // The catalog range alone takes 34 bytes (two 9-byte keys and their spans); a whole-list read of the 33-byte
    // forward key (the keyspace byte, then namespace, graph, predicate and subject as u64s, QuadKey's layout) takes 41
    // more, its key and its 8-byte span: 75 in all.
    let limits = TxnLimits::default().with_max_read_set(40);
    let mut txn = begin_with(&applier, Mode::ReadWrite, limits);
    objects(&mut txn, "a", "knows", 0).unwrap();
    let error = txn.commit(1, 1, &[], 0).unwrap_err();
    assert_eq!(error, TxnError::ReadSetTooLarge(75, 40));
    assert!(!error.is_retryable());
    // The read set's bound exactly and one below it.
    let read = |max: usize| {
        let mut txn = begin_with(
            &applier,
            Mode::ReadWrite,
            TxnLimits::default().with_max_read_set(max),
        );
        objects(&mut txn, "a", "knows", 0).unwrap();
        txn.commit(1, 1, &[], 0).map(|_| ())
    };
    assert_eq!(read(75), Ok(()));
    assert_eq!(read(74), Err(TxnError::ReadSetTooLarge(75, 74)));
}

#[test]
fn test_a_write_set_past_its_limit_refuses_the_commit() {
    let applier = applier();
    // The record the same transaction writes with the default bound: its length is the size a smaller bound refuses.
    let write = |max: usize| {
        let mut txn = begin_with(
            &applier,
            Mode::ReadWrite,
            TxnLimits::default().with_max_write_set(max),
        );
        txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
            .unwrap();
        txn.commit(1, 1, &[], 0)
    };
    let size = prepared(write(TxnLimits::default().max_write_set()))
        .record()
        .len();
    assert!(size > 200, "{size}");
    let error = write(200).unwrap_err();
    assert_eq!(error, TxnError::WriteSetTooLarge(size, 200));
    assert!(!error.is_retryable());
    // The bound exactly and one below it.
    assert_eq!(prepared(write(size)).record().len(), size);
    assert_eq!(
        write(size - 1).unwrap_err(),
        TxnError::WriteSetTooLarge(size, size - 1)
    );
    assert!(held(&applier, "a", "knows").is_empty());
}

#[test]
fn test_an_abort_gives_back_the_buffers_it_was_given() {
    let applier = applier();
    let buffers = TxnBuffers::with_capacity(4_096, 64, 8_192);
    let (read, record) = (buffers.read_set_capacity(), buffers.record_capacity());
    let mut txn = Transaction::begin(
        &applier,
        NS,
        Mode::ReadWrite,
        0,
        0,
        TxnLimits::default(),
        buffers,
    )
    .unwrap();
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .unwrap();
    let back = txn.abort();
    assert_eq!(
        (back.read_set_capacity(), back.record_capacity()),
        (read, record)
    );
}

#[test]
fn test_the_objects_bound_of_the_limits_names_a_list_whole() {
    // With a bound of no objects, the forward list an insert changes is a conflict point, not a statement.
    let applier = applier();
    commit(&applier, vec![insert("a", "knows", "c")]);
    let limits = TxnLimits::default().with_objects_per_list(0);
    let mut txn = begin_with(&applier, Mode::ReadWrite, limits);
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .unwrap();
    let prepared = prepared(txn.commit(1, 1, &[], 0));
    let view = WriteSetRecordView::new(prepared.record()).unwrap();
    assert!(view.conflict_statements().is_empty());
    let points: Vec<Vec<u8>> = view
        .conflict_points()
        .iter()
        .map(|e| e.key().to_vec())
        .collect();
    assert!(
        points.iter().any(|k| k.first() == Some(&0x10)),
        "the forward list is a point: {points:?}"
    );
}

#[test]
fn test_a_read_the_store_fails_ends_the_transaction() {
    let applier = applier();
    commit(&applier, vec![insert("a", "knows", "b")]);
    let mut txn = begin(&applier, Mode::ReadWrite);
    applier.quads().lists().store().inject(Fault::FailNextRead);
    assert!(matches!(
        objects(&mut txn, "a", "unread", 0),
        Err(TxnError(TxnErrorEnum::Changeset(_)))
    ));
    assert!(txn.is_ended());
    assert!(matches!(
        objects(&mut txn, "a", "knows", 0),
        Err(TxnError(TxnErrorEnum::Ended))
    ));
}

#[test]
fn test_the_buffers_are_reused_from_transaction_to_transaction() {
    let applier = applier();
    commit(&applier, vec![insert("a", "knows", "b")]);
    let run = |buffers: TxnBuffers| -> TxnBuffers {
        let mut txn = Transaction::begin(
            &applier,
            NS,
            Mode::ReadWrite,
            0,
            0,
            TxnLimits::default(),
            buffers,
        )
        .unwrap();
        objects(&mut txn, "a", "knows", 0).unwrap();
        txn.write(&changeset(vec![insert("a", "knows", "c")]), 0)
            .unwrap();
        prepared(txn.commit(1, 1, &[], 0)).into_parts().1
    };
    let buffers = run(TxnBuffers::with_capacity(4_096, 64, 8_192));
    let (read, record) = (buffers.read_set_capacity(), buffers.record_capacity());
    assert!(read.0 >= 4_096 && record >= 8_192);
    let buffers = run(buffers);
    assert_eq!(
        (buffers.read_set_capacity(), buffers.record_capacity()),
        (read, record)
    );
}

/// A limit whose microseconds pass `u64::MAX` is longer than any clock reading can show: 18,446,744,073,710 s is
/// 18,446,744,073,710,000,000 µs, 448,384 µs past 2^64 = 18,446,744,073,709,551,616, so a limit cut to u64 would end a
/// transaction after 0.45 s. A request and a commit 1 s after the begin are within it, in either mode, and so is the
/// largest clock reading with `Duration::MAX`.
#[test]
fn test_a_limit_past_u64_microseconds_is_not_cut_short() {
    let applier = applier();
    commit(&applier, vec![insert("a", "knows", "b")]);
    for limit in [Duration::from_secs(18_446_744_073_710), Duration::MAX] {
        let limits = TxnLimits::default()
            .with_read_write(limit)
            .with_read_only(limit);
        let mut txn = begin_with(&applier, Mode::ReadWrite, limits);
        assert_eq!(
            objects(&mut txn, "a", "knows", SECOND),
            Ok(vec![Term::from(ex("b"))]),
            "{limit:?}"
        );
        txn.write(&changeset(vec![insert("a", "knows", "c")]), SECOND)
            .unwrap();
        prepared(txn.commit(1, 1, &[], SECOND));
        let mut read_only = begin_with(&applier, Mode::ReadOnly, limits);
        assert_eq!(
            objects(&mut read_only, "a", "knows", u64::MAX),
            Ok(vec![Term::from(ex("b"))]),
            "{limit:?}"
        );
        assert!(matches!(
            read_only.commit(0, 0, &[], u64::MAX),
            Ok(Commit::ReadOnly(_))
        ));
    }
}
