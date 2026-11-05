/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;
use ultragraphdb_tool_bdd::{Context, TestResult};

use oxrdf::{Literal, Term};
use ultragraphdb_core_mapping::{Change, Changeset, Declaration, PredicateDeclaration};
use ultragraphdb_protocol_wire::WriteSetRecordView;
use ultragraphdb_store_apply::{ChangesetError, ChangesetErrorEnum};
use ultragraphdb_store_sim::Fault;
use ultragraphdb_transactions_txn::{
    Commit, Mode, Prepared, Transaction, TxnBuffers, TxnError, TxnErrorEnum, TxnLimits,
};

use crate::utils::fixture::{
    SECOND, TENANT, applier, begin, begin_with, changeset, commit, ex, held, insert,
    insert_literal, objects, prepared, read_objects, store,
};

fn literal(value: &str) -> Term {
    Literal::new_simple_literal(value).into()
}

#[test]
fn test_a_transaction_begins_at_the_watermark() -> TestResult {
    let applier = applier()?;
    let watermark = commit(&applier, vec![insert_literal("a", "name", "A")])?;
    let txn = begin(&applier, Mode::ReadWrite)?;
    assert_eq!(txn.read_version(), watermark);
    assert_eq!(txn.tenant(), TENANT);
    assert_eq!(txn.mode(), Mode::ReadWrite);
    assert!(!txn.is_ended());
    Ok(())
}

#[test]
fn test_a_named_version_at_or_below_the_watermark_is_read() -> TestResult {
    let applier = applier()?;
    let first = commit(&applier, vec![insert_literal("a", "name", "A")])?;
    let second = commit(&applier, vec![insert_literal("a", "name", "B")])?;
    assert!(first < second);
    for (version, expected) in [
        (first, vec![literal("A")]),
        (second, vec![literal("A"), literal("B")]),
    ] {
        let mut txn = Transaction::begin(
            &applier,
            TENANT,
            Mode::ReadOnly,
            version,
            0,
            TxnLimits::default(),
            TxnBuffers::new(),
        )
        .context("at or below the watermark")?;
        assert_eq!(txn.read_version(), version);
        let mut read = objects(&mut txn, "a", "name", 0)?;
        read.sort_by_key(|t| t.to_string());
        assert_eq!(read, expected);
    }
    Ok(())
}

#[test]
fn test_a_version_above_the_watermark_is_refused_at_once_naming_both() -> TestResult {
    let applier = applier()?;
    let watermark = commit(&applier, vec![insert_literal("a", "name", "A")])?;
    for above in [watermark + 1, watermark + 2, u64::MAX] {
        let refused = Transaction::begin(
            &applier,
            TENANT,
            Mode::ReadWrite,
            above,
            0,
            TxnLimits::default(),
            TxnBuffers::new(),
        );
        assert_eq!(
            refused.err(),
            Some(TxnError::read_above_watermark(above, watermark))
        );
    }
    // The worker goes on: the next transaction begins and reads.
    let mut next = begin(&applier, Mode::ReadOnly)?;
    assert_eq!(objects(&mut next, "a", "name", 0)?, vec![literal("A")]);
    Ok(())
}

#[test]
fn test_a_commit_after_the_transaction_began_is_not_visible_to_it() -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert_literal("a", "name", "A")])?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    store(&applier, vec![insert_literal("b", "name", "B")])?;
    // `b` was never read before the commit, so the snapshot, not a cache, keeps it out.
    assert!(objects(&mut txn, "b", "name", 0)?.is_empty());
    assert_eq!(held(&applier, "b", "name")?, vec![literal("B")]);
    Ok(())
}

#[test]
fn test_a_read_sees_the_transactions_own_writes_across_requests_and_no_one_elses() -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert("a", "knows", "z")])?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)?;
    txn.write(&changeset(vec![insert("a", "knows", "c")]), 0)?;
    let mut read: Vec<String> = objects(&mut txn, "a", "knows", 0)?
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
    txn.write(&changeset(vec![delete]), 0)?;
    assert_eq!(objects(&mut txn, "a", "knows", 0)?.len(), 2);
    // Nobody else sees them.
    assert_eq!(held(&applier, "a", "knows")?, vec![Term::from(ex("z"))]);
    Ok(())
}

#[test]
fn test_a_subject_the_transaction_created_reads_back_with_its_new_terms() -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    txn.write(
        &changeset(vec![
            insert("new", "knows", "other"),
            insert_literal("new", "name", "fresh"),
        ]),
        0,
    )?;
    assert_eq!(
        objects(&mut txn, "new", "knows", 0)?,
        vec![Term::from(ex("other"))]
    );
    assert_eq!(objects(&mut txn, "new", "name", 0)?, vec![literal("fresh")]);
    Ok(())
}

#[test]
fn test_a_read_of_terms_the_store_lacks_holds_nothing_and_records_them_absent() -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert("a", "knows", "b")])?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    assert!(objects(&mut txn, "nobody", "knows", 0)?.is_empty());
    assert!(objects(&mut txn, "a", "unknown", 0)?.is_empty());
    let prepared = prepared(txn.commit(1, 1, &[], 0))?;
    // A term the store lacks takes an ID while the read runs (store_quads' get-or-create), recorded absent by it.
    let absent: Vec<Vec<u8>> = prepared.read_set().absent().map(<[u8]>::to_vec).collect();
    let executed_absent: Vec<Vec<u8>> = prepared
        .executed()
        .reads()
        .absent()
        .iter()
        .map(|id| ultragraphdb_transactions_txn::ReadSet::term_key(*id).to_vec())
        .collect();
    assert_eq!(absent.len(), 2, "nobody and unknown: {absent:?}");
    assert!(executed_absent.iter().all(|k| absent.contains(k)));
    assert!(absent.iter().all(|k| k.first() == Some(&0xFE)));
    Ok(())
}

#[test]
fn test_a_read_write_transactions_reads_are_in_its_read_set() -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert("a", "knows", "b")])?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    read_objects(&mut txn, "a", "knows", 0)?;
    txn.write(&changeset(vec![insert("c", "knows", "b")]), 0)?;
    let watermark = txn.read_version();
    let prepared = prepared(txn.with_catalog(4).commit(5, 6, b"cr", 0))?;
    let set = prepared.read_set();
    assert!(set.is_sealed());
    assert_eq!((set.tenant(), set.read_version()), (TENANT, watermark));
    let a = applier
        .quads()
        .id_of(TENANT, ex("a").as_str())?
        .context("a's ID")?;
    let knows = applier
        .quads()
        .id_of(TENANT, ex("knows").as_str())?
        .context("knows's ID")?;
    let forward = ultragraphdb_core_keys::QuadKey::Forward {
        tenant: TENANT,
        graph: ultragraphdb_core_keys::DEFAULT_GRAPH,
        predicate: knows,
        subject: a,
    }
    .to_bytes();
    assert!(
        set.lists().any(|k| k == forward.as_slice()),
        "the list read whole"
    );
    // The record carries the transaction's identity and writes.
    assert_eq!(prepared.tenant(), TENANT);
    assert_eq!(prepared.read_version(), watermark);
    let view = WriteSetRecordView::new(prepared.record())?;
    assert_eq!((view.tenant(), view.read_version()), (TENANT, watermark));
    assert_eq!((view.session(), view.request()), (5u128.to_be_bytes(), 6));
    assert_eq!(view.commit_record(), b"cr");
    assert_eq!(view.catalog(), 4, "the catalog version it executed against");
    assert_eq!(view.inserted().iter().count(), 1);
    assert_eq!(prepared.executed().read_ts(), watermark);
    Ok(())
}

#[test]
fn test_a_read_write_transaction_with_no_writes_is_still_validated() -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert("a", "knows", "b")])?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    read_objects(&mut txn, "a", "knows", 0)?;
    let prepared = prepared(txn.commit(1, 1, &[], 0))?;
    // The subject's predicate list, which names `knows`, then the list of `<a> <knows>`.
    assert_eq!(prepared.read_set().lists().count(), 2);
    let view = WriteSetRecordView::new(prepared.record())?;
    assert_eq!(view.inserted().iter().count(), 0);
    assert_eq!(view.removed().iter().count(), 0);
    Ok(())
}

#[test]
fn test_a_read_only_transaction_ends_on_its_worker_and_refuses_writes() -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadOnly)?;
    assert_eq!(
        txn.write(&changeset(vec![insert("a", "knows", "b")]), 0),
        Err(TxnError::read_only())
    );
    assert!(!txn.is_ended(), "a refused write leaves it as it was");
    assert!(objects(&mut txn, "a", "knows", 0)?.is_empty());
    assert!(matches!(txn.commit(0, 0, &[], 0), Ok(Commit::ReadOnly(_))));
    assert!(held(&applier, "a", "knows")?.is_empty());
    Ok(())
}

#[test]
fn test_a_changeset_of_another_tenant_is_refused_and_leaves_the_transaction_as_it_was() -> TestResult
{
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    let other = Changeset::new(3).with(insert("a", "knows", "b"));
    assert_eq!(txn.write(&other, 0), Err(TxnError::tenant(TENANT, 3)));
    assert!(!txn.is_ended());
    txn.write(&changeset(vec![insert("a", "knows", "c")]), 0)?;
    assert_eq!(
        objects(&mut txn, "a", "knows", 0)?,
        vec![Term::from(ex("c"))]
    );
    Ok(())
}

#[test]
fn test_a_refused_changeset_ends_the_transaction() -> TestResult {
    let applier = applier()?;
    // `age` declared an integer: a string object is refused by the catalog.
    let declared =
        PredicateDeclaration::new(ex("age")).with_datatype(oxrdf::vocab::xsd::INTEGER.into_owned());
    store(
        &applier,
        vec![Change::Declare(Declaration::Predicate(declared))],
    )?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
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
    assert!(!refused.err().context("the call fails")?.is_retryable());
    assert!(txn.is_ended());
    assert_eq!(
        txn.write(&changeset(vec![insert("a", "knows", "b")]), 0),
        Err(TxnError::ended())
    );
    assert_eq!(objects(&mut txn, "a", "knows", 0), Err(TxnError::ended()));
    assert!(matches!(txn.commit(1, 1, &[], 0), Err(e) if e == TxnError::ended()));
    Ok(())
}

#[test]
fn test_a_read_write_transaction_lives_exactly_its_limit() -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 10 * SECOND)?;
    read_objects(&mut txn, "a", "knows", 10 * SECOND)?;
    let prepared = prepared(txn.commit(1, 1, &[], 10 * SECOND))?;
    // The positive control of "nothing it wrote is held" below: a commit within the limit hands the commit step an
    // execution that, applied, is held.
    let (executed, _) = prepared.into_parts();
    let version = applier.quads().lists().next_version();
    assert_eq!(applier.apply_round(vec![(version, executed)])?.len(), 1);
    assert_eq!(held(&applier, "a", "knows")?, vec![Term::from(ex("b"))]);
    Ok(())
}

#[test]
fn test_one_microsecond_past_the_limit_refuses_each_request_and_ends_the_transaction() -> TestResult
{
    let applier = applier()?;
    let past = 10 * SECOND + 1;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    assert_eq!(
        txn.write(&changeset(vec![insert("a", "knows", "b")]), past),
        Err(TxnError::past_limit(10_000))
    );
    assert!(txn.is_ended());
    assert_eq!(
        objects(&mut txn, "a", "knows", past),
        Err(TxnError::ended())
    );

    let mut reading = begin(&applier, Mode::ReadWrite)?;
    assert_eq!(
        objects(&mut reading, "a", "knows", past),
        Err(TxnError::past_limit(10_000))
    );
    assert!(reading.is_ended());

    let mut committing = begin(&applier, Mode::ReadWrite)?;
    committing.write(&changeset(vec![insert("a", "knows", "b")]), 0)?;
    let error = committing
        .commit(1, 1, &[], past)
        .err()
        .context("the call fails")?;
    assert_eq!(error, TxnError::past_limit(10_000));
    assert!(error.is_retryable());
    assert!(held(&applier, "a", "knows")?.is_empty());
    Ok(())
}

#[test]
fn test_a_read_only_transaction_lives_sixteen_seconds() -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadOnly)?;
    read_objects(&mut txn, "a", "knows", 16 * SECOND)?;
    assert!(matches!(
        txn.commit(0, 0, &[], 16 * SECOND),
        Ok(Commit::ReadOnly(_))
    ));
    let mut later = begin(&applier, Mode::ReadOnly)?;
    read_objects(&mut later, "a", "knows", 11 * SECOND)?;
    assert_eq!(
        later
            .commit(0, 0, &[], 16 * SECOND + 1)
            .err()
            .context("the call fails")?,
        TxnError::past_limit(16_000)
    );
    Ok(())
}

#[test]
fn test_the_limits_are_settings_and_count_from_the_transactions_begin() -> TestResult {
    let applier = applier()?;
    let limits = TxnLimits::default()
        .with_read_write(Duration::from_millis(1_500))
        .with_read_only(Duration::from_millis(250));
    let began = 5 * SECOND;
    let mut txn = Transaction::begin(
        &applier,
        TENANT,
        Mode::ReadWrite,
        0,
        began,
        limits,
        TxnBuffers::new(),
    )?;
    read_objects(&mut txn, "a", "knows", began + 1_500_000)?;
    assert_eq!(
        objects(&mut txn, "a", "knows", began + 1_500_001),
        Err(TxnError::past_limit(1_500))
    );
    let mut read_only = Transaction::begin(
        &applier,
        TENANT,
        Mode::ReadOnly,
        0,
        began,
        limits,
        TxnBuffers::new(),
    )?;
    assert_eq!(
        objects(&mut read_only, "a", "knows", began + 250_001),
        Err(TxnError::past_limit(250))
    );
    Ok(())
}

#[test]
fn test_a_clock_behind_the_begin_is_no_time_passed() -> TestResult {
    let applier = applier()?;
    let mut txn = Transaction::begin(
        &applier,
        TENANT,
        Mode::ReadWrite,
        0,
        20 * SECOND,
        TxnLimits::default(),
        TxnBuffers::new(),
    )?;
    read_objects(&mut txn, "a", "knows", SECOND)?;
    assert!(!txn.is_ended());
    Ok(())
}

#[test]
fn test_an_aborted_transaction_writes_nothing_and_returns_its_buffers() -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)?;
    let buffers = txn.abort();
    assert!(held(&applier, "a", "knows")?.is_empty());
    let mut again = Transaction::begin(
        &applier,
        TENANT,
        Mode::ReadWrite,
        0,
        0,
        TxnLimits::default(),
        buffers,
    )?;
    assert!(objects(&mut again, "a", "knows", 0)?.is_empty());
    Ok(())
}

#[test]
fn test_a_read_set_past_its_limit_refuses_the_commit() -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert("a", "knows", "b")])?;
    // A whole-list read of the 57-byte forward key (the keyspace byte, the tenant's 8 bytes, then graph, predicate
    // and subject of 16 bytes each, QuadKey's layout) takes its key and its 8-byte span: 65; the subject-predicates list
    // read before it (partitions, "Absence is proven by a node's predicate lists"), its 41-byte key (the keyspace byte,
    // the tenant's 8 bytes, graph and subject of 16 each) and its span, 49 more: 114 in all.
    let limits = TxnLimits::default().with_max_read_set(40);
    let mut txn = begin_with(&applier, Mode::ReadWrite, limits)?;
    read_objects(&mut txn, "a", "knows", 0)?;
    let error = txn.commit(1, 1, &[], 0).err().context("the call fails")?;
    assert_eq!(error, TxnError::read_set_too_large(114, 40));
    assert!(!error.is_retryable());
    // The read set's bound exactly and one below it.
    let read = |max: usize| -> TestResult<Result<(), TxnError>> {
        let mut bounded = begin_with(
            &applier,
            Mode::ReadWrite,
            TxnLimits::default().with_max_read_set(max),
        )?;
        read_objects(&mut bounded, "a", "knows", 0)?;
        Ok(bounded.commit(1, 1, &[], 0).map(|_| ()))
    };
    assert_eq!(read(114)?, Ok(()));
    assert_eq!(read(113)?, Err(TxnError::read_set_too_large(114, 113)));
    Ok(())
}

#[test]
fn test_a_write_set_past_its_limit_refuses_the_commit() -> TestResult {
    let applier = applier()?;
    // The write set the same transaction makes with the default bound, its record and the dictionary entries its round
    // holds for it: their size is what a smaller bound refuses.
    let write = |max: usize| -> TestResult<Result<Commit, TxnError>> {
        let mut txn = begin_with(
            &applier,
            Mode::ReadWrite,
            TxnLimits::default().with_max_write_set(max),
        )?;
        txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)?;
        Ok(txn.commit(1, 1, &[], 0))
    };
    let unbounded = prepared(write(TxnLimits::default().max_write_set())?)?;
    let record = unbounded.record().len();
    let size = record
        + ultragraphdb_transactions_txn::WriteSetBuilder::dictionary_len(unbounded.executed());
    assert!(size > record, "the new IRIs' entries count");
    assert!(size > 200, "{size}");
    let error = write(200)?.err().context("the call fails")?;
    assert_eq!(error, TxnError::write_set_too_large(size, 200));
    assert!(!error.is_retryable());
    // The bound exactly and one below it.
    assert_eq!(prepared(write(size)?)?.record().len(), record);
    assert_eq!(
        write(size - 1)?.err().context("the call fails")?,
        TxnError::write_set_too_large(size, size - 1)
    );
    assert!(held(&applier, "a", "knows")?.is_empty());
    Ok(())
}

#[test]
fn test_an_abort_gives_back_the_buffers_it_was_given() -> TestResult {
    let applier = applier()?;
    let buffers = TxnBuffers::with_capacity(4_096, 64, 8_192);
    let (read, record) = (buffers.read_set_capacity(), buffers.record_capacity());
    let mut txn = Transaction::begin(
        &applier,
        TENANT,
        Mode::ReadWrite,
        0,
        0,
        TxnLimits::default(),
        buffers,
    )?;
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)?;
    let back = txn.abort();
    assert_eq!(
        (back.read_set_capacity(), back.record_capacity()),
        (read, record)
    );
    Ok(())
}

#[test]
fn test_the_objects_bound_of_the_limits_names_a_list_whole() -> TestResult {
    // With a bound of no objects, the forward list an insert changes is a conflict point, not a statement.
    let applier = applier()?;
    store(&applier, vec![insert("a", "knows", "c")])?;
    let limits = TxnLimits::default().with_objects_per_list(0);
    let mut txn = begin_with(&applier, Mode::ReadWrite, limits)?;
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)?;
    let prepared = prepared(txn.commit(1, 1, &[], 0))?;
    let view = WriteSetRecordView::new(prepared.record())?;
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
    Ok(())
}

#[test]
fn test_a_read_the_store_fails_ends_the_transaction() -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert("a", "knows", "b")])?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    applier.quads().lists().store().inject(Fault::FailNextRead);
    let refused = objects(&mut txn, "a", "unread", 0);
    assert!(
        matches!(
            refused,
            Err(TxnError(TxnErrorEnum::Changeset(
                ultragraphdb_store_apply::ChangesetError(
                    ultragraphdb_store_apply::ChangesetErrorEnum::Quad(_)
                )
            )))
        ) && refused
            .as_ref()
            .is_err_and(|e| e.to_string().contains("simulated read failure")),
        "the store's failure: {refused:?}"
    );
    assert!(txn.is_ended());
    assert!(matches!(
        objects(&mut txn, "a", "knows", 0),
        Err(TxnError(TxnErrorEnum::Ended))
    ));
    Ok(())
}

#[test]
fn test_the_buffers_are_reused_from_transaction_to_transaction() -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert("a", "knows", "b")])?;
    let run = |buffers: TxnBuffers| -> TestResult<TxnBuffers> {
        let mut txn = Transaction::begin(
            &applier,
            TENANT,
            Mode::ReadWrite,
            0,
            0,
            TxnLimits::default(),
            buffers,
        )?;
        read_objects(&mut txn, "a", "knows", 0)?;
        txn.write(&changeset(vec![insert("a", "knows", "c")]), 0)?;
        Ok(prepared(txn.commit(1, 1, &[], 0))?.into_parts().1)
    };
    let buffers = run(TxnBuffers::with_capacity(4_096, 64, 8_192))?;
    let (read, record) = (buffers.read_set_capacity(), buffers.record_capacity());
    assert!(read.0 >= 4_096 && record >= 8_192);
    let buffers = run(buffers)?;
    assert_eq!(
        (buffers.read_set_capacity(), buffers.record_capacity()),
        (read, record)
    );
    Ok(())
}

/// A limit whose microseconds pass `u64::MAX` is longer than any clock reading can show: 18,446,744,073,710 s is
/// 18,446,744,073,710,000,000 µs, 448,384 µs past 2^64 = 18,446,744,073,709,551,616, so a limit cut to u64 would end a
/// transaction after 0.45 s. A request and a commit 1 s after the begin are within it, in either mode, and so is the
/// largest clock reading with `Duration::MAX`.
#[test]
fn test_a_limit_past_u64_microseconds_is_not_cut_short() -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert("a", "knows", "b")])?;
    for limit in [Duration::from_secs(18_446_744_073_710), Duration::MAX] {
        let limits = TxnLimits::default()
            .with_read_write(limit)
            .with_read_only(limit);
        let mut txn = begin_with(&applier, Mode::ReadWrite, limits)?;
        assert_eq!(
            objects(&mut txn, "a", "knows", SECOND),
            Ok(vec![Term::from(ex("b"))]),
            "{limit:?}"
        );
        txn.write(&changeset(vec![insert("a", "knows", "c")]), SECOND)?;
        let _proposal: Prepared = prepared(txn.commit(1, 1, &[], SECOND))?;
        let mut read_only = begin_with(&applier, Mode::ReadOnly, limits)?;
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
    Ok(())
}

#[test]
fn test_a_transaction_of_another_tenant_names_it_in_its_proposal() -> TestResult {
    // Tenant 7, not the fixture's 0: the transaction, its proposal, its read set and its record all name it.
    let applier = applier()?;
    let mut txn = Transaction::begin(
        &applier,
        7,
        Mode::ReadWrite,
        0,
        0,
        TxnLimits::default(),
        TxnBuffers::new(),
    )
    .context("it begins")?;
    assert_eq!(txn.tenant(), 7);
    txn.write(&Changeset::new(7).with(insert("a", "knows", "b")), 0)?;
    let prepared = prepared(txn.commit(1, 1, &[], 0))?;
    assert_eq!(prepared.tenant(), 7);
    assert_eq!(prepared.read_set().tenant(), 7);
    let view = WriteSetRecordView::new(prepared.record())?;
    assert_eq!(view.tenant(), 7);
    Ok(())
}

/// The name `s{i}` of the first `i` whose IRI's bucket `placement` gives to `partition` at epoch 0: the placement map
/// and the bucket derivation are the oracle.
fn owned_by(
    placement: &ultragraphdb_cluster_placement::PlacementMap,
    partition: u32,
) -> TestResult<String> {
    use ultragraphdb_core_ids::{Bucket, CanonicalKey, Epoch, Tenant, TypePrefix};
    for i in 0..1_000 {
        let name = format!("s{i}");
        let iri = ex(&name);
        let key = CanonicalKey::iri(iri.as_str()).context("the key")?;
        let bucket = Bucket::derive(Tenant::new(TENANT), TypePrefix::RESOURCE, &key);
        if placement.partition_of(bucket, Epoch::ZERO).get() == partition {
            return Ok(name);
        }
    }
    Err(ultragraphdb_tool_bdd::TestError::new(format!(
        "no name of partition {partition}"
    )))
}

fn two_partitions() -> TestResult<ultragraphdb_cluster_placement::PlacementMap> {
    ultragraphdb_cluster_placement::PlacementMap::even(
        2,
        ultragraphdb_cluster_placement::NodeId::new(1),
    )
    .context("the placement")
}

fn partition(n: u32) -> TestResult<ultragraphdb_cluster_placement::PartitionId> {
    ultragraphdb_cluster_placement::PartitionId::new(n).context("the partition")
}

#[test]
fn test_a_bound_transaction_writes_its_own_partitions_subjects() -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let own = owned_by(&placement, 1)?;
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &placement);
    txn.write(&changeset(vec![insert(&own, "knows", "b")]), 0)?;
    assert!(!txn.is_ended());
    let prepared = prepared(txn.commit(1, 1, &[], 0))?;
    assert_eq!(
        WriteSetRecordView::new(prepared.record())?
            .inserted()
            .iter()
            .count(),
        1
    );
    Ok(())
}

#[test]
fn test_a_write_of_another_partitions_subject_aborts_cross_partition() -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let other = owned_by(&placement, 2)?;
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &placement);
    let error = txn
        .write(&changeset(vec![insert(&other, "knows", "b")]), 0)
        .err()
        .context("the write is refused")?;
    assert_eq!(error, TxnError::cross_partition(1, 2));
    assert_eq!(
        error.0,
        TxnErrorEnum::CrossPartition {
            partition: 1,
            other: 2
        }
    );
    assert!(!error.is_retryable());
    assert!(txn.is_ended(), "it is aborted");
    assert!(matches!(txn.commit(1, 1, &[], 0), Err(e) if e == TxnError::ended()));
    assert!(
        held(&applier, &other, "knows")?.is_empty(),
        "nothing is held"
    );
    Ok(())
}

#[test]
fn test_one_foreign_subject_among_owned_ones_aborts_the_whole_write() -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let (own, other) = (owned_by(&placement, 2)?, owned_by(&placement, 1)?);
    // Bound to partition 2 this time: the other partition is 1.
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(2)?, &placement);
    let error = txn
        .write(
            &changeset(vec![
                insert(&own, "knows", "b"),
                insert(&other, "knows", "b"),
            ]),
            0,
        )
        .err()
        .context("the write is refused")?;
    assert_eq!(error, TxnError::cross_partition(2, 1));
    assert!(txn.is_ended());
    Ok(())
}

#[test]
fn test_an_unbound_transaction_writes_every_subject() -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let (one, two) = (owned_by(&placement, 1)?, owned_by(&placement, 2)?);
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    txn.write(
        &changeset(vec![insert(&one, "knows", "b"), insert(&two, "knows", "b")]),
        0,
    )?;
    assert!(!txn.is_ended());
    Ok(())
}

#[test]
fn test_a_new_blank_node_subject_is_the_bound_partitions_own() -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(2)?, &placement);
    let blank = Change::Insert {
        graph: oxrdf::GraphName::DefaultGraph,
        subject: oxrdf::BlankNode::new_unchecked("fresh").into(),
        predicate: ex("knows"),
        object: ex("b").into(),
    };
    txn.write(&changeset(vec![blank]), 0)?;
    assert!(
        !txn.is_ended(),
        "the partition draws a new blank node's bucket"
    );
    Ok(())
}

#[test]
fn test_a_transaction_carries_its_token_and_catalog_version_into_its_proposal() -> TestResult {
    use ultragraphdb_core_ids::{Epoch, SessionToken};
    let applier = applier()?;
    let token = SessionToken::new(3, 40, Epoch::new(12).context("the epoch")?);
    let txn = begin(&applier, Mode::ReadWrite)?
        .with_token(token.clone())
        .with_catalog(9);
    assert_eq!(txn.token(), Some(&token));
    assert_eq!(txn.catalog(), 9);
    let prepared = prepared(txn.commit(1, 1, &[], 0))?;
    assert_eq!(prepared.token(), Some(&token));
    assert_eq!(WriteSetRecordView::new(prepared.record())?.catalog(), 9);
    Ok(())
}

#[test]
fn test_a_transaction_carries_no_token_and_catalog_0_by_default() -> TestResult {
    let applier = applier()?;
    let txn = begin(&applier, Mode::ReadWrite)?;
    assert_eq!(txn.token(), None);
    assert_eq!(txn.catalog(), 0);
    let prepared = prepared(txn.commit(1, 1, &[], 0))?;
    assert_eq!(prepared.token(), None);
    assert_eq!(WriteSetRecordView::new(prepared.record())?.catalog(), 0);
    Ok(())
}

/// A delete of a statement, or of every object of a predicate, whose subject another partition owns aborts a bound
/// transaction cross-partition, as an insert does: each writes the subject's lists.
#[test]
fn test_a_delete_of_another_partitions_subject_aborts_cross_partition() -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let other = owned_by(&placement, 2)?;
    store(&applier, vec![insert(&other, "knows", "b")])?;
    let deletes = [
        Change::Delete {
            graph: oxrdf::GraphName::DefaultGraph,
            subject: ex(&other).into(),
            predicate: ex("knows"),
            object: ex("b").into(),
        },
        Change::DeletePattern {
            graph: oxrdf::GraphName::DefaultGraph,
            subject: ex(&other).into(),
            predicate: ex("knows"),
        },
    ];
    for delete in deletes {
        let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &placement);
        let error = txn
            .write(&changeset(vec![delete.clone()]), 0)
            .err()
            .context("the delete is refused")?;
        assert_eq!(error, TxnError::cross_partition(1, 2), "{delete:?}");
        assert!(txn.is_ended(), "{delete:?}");
    }
    assert_eq!(
        held(&applier, &other, "knows")?,
        vec![Term::from(ex("b"))],
        "nothing is deleted"
    );
    Ok(())
}

/// A property set on, or removed from, a statement named by its triple whose subject another partition owns aborts a
/// bound transaction cross-partition: the statement is the subject's, and setting a property gives it a statement ID in
/// the subject's forward list.
#[test]
fn test_a_property_of_another_partitions_statement_aborts_cross_partition() -> TestResult {
    use ultragraphdb_core_mapping::StatementRef;
    let applier = applier()?;
    let placement = two_partitions()?;
    let other = owned_by(&placement, 2)?;
    store(&applier, vec![insert(&other, "knows", "b")])?;
    let statement = || StatementRef::Triple {
        graph: oxrdf::GraphName::DefaultGraph,
        subject: ex(&other).into(),
        predicate: ex("knows"),
        object: ex("b").into(),
    };
    let changes = [
        Change::SetProperty {
            statement: statement(),
            predicate: ex("since"),
            value: Literal::from(2020i64).into(),
        },
        Change::DeleteProperty {
            statement: statement(),
            predicate: ex("since"),
            value: None,
        },
    ];
    for change in changes {
        let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &placement);
        let error = txn
            .write(&changeset(vec![change.clone()]), 0)
            .err()
            .context("the write is refused")?;
        assert_eq!(error, TxnError::cross_partition(1, 2), "{change:?}");
        assert!(txn.is_ended(), "{change:?}");
    }
    Ok(())
}

/// `<subject> <knows> <b>` held as an edge with the property `<since> 2020`: its statement ID and the reifier naming it.
fn edge(
    applier: &crate::utils::fixture::Applier,
    subject: &str,
) -> TestResult<(ultragraphdb_core_ids::SystemId, oxrdf::NamedNode)> {
    use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
    store(
        applier,
        vec![Change::InsertEdge {
            graph: oxrdf::GraphName::DefaultGraph,
            subject: ex(subject).into(),
            predicate: ex("knows"),
            object: ex("b").into(),
            properties: vec![(ex("since"), Literal::from(2020i64).into())],
        }],
    )?;
    let quads = applier.quads();
    let id = |name: &str| -> TestResult<ultragraphdb_core_ids::SystemId> {
        quads
            .id_of(crate::utils::fixture::TENANT, ex(name).as_str())?
            .context("the IRI is held")
    };
    let key = QuadKey::Forward {
        tenant: crate::utils::fixture::TENANT,
        graph: DEFAULT_GRAPH,
        predicate: id("knows")?,
        subject: id(subject)?,
    }
    .to_bytes();
    let statement = quads
        .objects(&key, quads.lists().applied_ts())?
        .first()
        .and_then(|(_, ids)| ids.first().copied())
        .context("the edge's statement ID")?;
    let reifier = oxrdf::NamedNode::new(quads.iri_of(crate::utils::fixture::TENANT, statement)?.0)?;
    Ok((statement, reifier))
}

/// The properties statement `id` holds now.
fn properties_of(
    applier: &crate::utils::fixture::Applier,
    id: ultragraphdb_core_ids::SystemId,
) -> TestResult<usize> {
    let record = applier.quads().statement(
        crate::utils::fixture::TENANT,
        ultragraphdb_core_keys::DEFAULT_GRAPH,
        id,
        applier.quads().lists().applied_ts(),
    )?;
    Ok(record.context("the statement's record")?.properties().len())
}

/// The three writes naming a statement by `reifier`: a property set, a property removed, and the statement deleted.
fn by_reifier(reifier: &oxrdf::NamedNode) -> [Change; 3] {
    use ultragraphdb_core_mapping::StatementRef;
    let statement = || StatementRef::Reifier {
        graph: oxrdf::GraphName::DefaultGraph,
        reifier: reifier.clone(),
    };
    [
        Change::SetProperty {
            statement: statement(),
            predicate: ex("weight"),
            value: Literal::from(1i64).into(),
        },
        Change::DeleteProperty {
            statement: statement(),
            predicate: ex("since"),
            value: None,
        },
        Change::DeleteStatement {
            graph: oxrdf::GraphName::DefaultGraph,
            reifier: reifier.clone(),
        },
    ]
}

/// A statement named by its reifier whose subject the bound partition owns is written: each write is taken, the
/// transaction goes on, and its record names the statement's record.
#[test]
fn test_a_statement_named_by_its_reifier_in_the_bound_partition_is_written() -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let own = owned_by(&placement, 1)?;
    let (_, reifier) = edge(&applier, &own)?;
    for change in by_reifier(&reifier) {
        let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &placement);
        txn.write(&changeset(vec![change.clone()]), 0)?;
        assert!(!txn.is_ended(), "{change:?}");
        let proposal = prepared(txn.commit(1, 1, &[], 0))?;
        assert_eq!(
            WriteSetRecordView::new(proposal.record())?
                .statement_records()
                .iter()
                .count(),
            1,
            "{change:?}"
        );
        let _buffers = proposal.release(&applier);
    }
    Ok(())
}

/// A statement named by its reifier whose subject another partition owns aborts the bound transaction cross-partition
/// once the reifier resolves, naming both partitions, and writes nothing: the statement keeps its edge and property,
/// and the ended transaction commits nothing.
#[test]
fn test_a_statement_named_by_its_reifier_in_another_partition_aborts_cross_partition() -> TestResult
{
    let applier = applier()?;
    let placement = two_partitions()?;
    let other = owned_by(&placement, 2)?;
    let (id, reifier) = edge(&applier, &other)?;
    let watermark = applier.quads().lists().applied_ts();
    for change in by_reifier(&reifier) {
        let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &placement);
        let error = txn
            .write(&changeset(vec![change.clone()]), 0)
            .err()
            .context("the write is refused")?;
        assert_eq!(error, TxnError::cross_partition(1, 2), "{change:?}");
        assert!(!error.is_retryable());
        assert!(txn.is_ended(), "{change:?}");
        assert_eq!(txn.commit(1, 1, &[], 0).err(), Some(TxnError::ended()));
    }
    assert_eq!(applier.quads().lists().applied_ts(), watermark);
    assert_eq!(held(&applier, &other, "knows")?, vec![Term::from(ex("b"))]);
    assert_eq!(properties_of(&applier, id)?, 1, "`<since>` alone");
    Ok(())
}

/// A reifier naming no statement resolves to no subject: a delete of it is taken and writes nothing, and a property
/// set through it is refused as naming no statement, ending the transaction.
#[test]
fn test_a_reifier_naming_no_statement_is_no_cross_partition_write() -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    store(&applier, vec![insert("a", "knows", "b")])?;
    let [set, _, delete] = by_reifier(&ex("nothing"));
    let mut txn = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &placement);
    txn.write(&changeset(vec![delete]), 0)?;
    assert!(!txn.is_ended());
    let proposal = prepared(txn.commit(1, 1, &[], 0))?;
    let view = WriteSetRecordView::new(proposal.record())?;
    assert_eq!(
        (
            view.inserted().iter().count(),
            view.removed().iter().count(),
            view.statement_records().iter().count()
        ),
        (0, 0, 0)
    );
    let _buffers = proposal.release(&applier);
    let mut setter = begin(&applier, Mode::ReadWrite)?.bind(partition(1)?, &placement);
    let refused = setter
        .write(&changeset(vec![set]), 0)
        .err()
        .context("the property is refused")?;
    assert!(
        matches!(
            &refused,
            TxnError(TxnErrorEnum::Changeset(ChangesetError(
                ChangesetErrorEnum::Mapping(_)
            )))
        ),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "No statement <http://ex.org/nothing> in DEFAULT is held"
    );
    assert!(setter.is_ended());
    Ok(())
}
