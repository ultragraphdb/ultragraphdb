/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::WriteSetRecordView;
use ultragraphdb_transactions_txn::{Commit, Mode};

use crate::utils::fixture::{NS, applier, begin, changeset, insert};

#[test]
fn test_its_parts_are_the_execution_and_the_buffers() {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadWrite);
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .unwrap();
    let Ok(Commit::ReadWrite(prepared)) = txn.commit(1, 2, &[], 0) else {
        panic!("a read-write commit");
    };
    let record_len = prepared.record().len();
    let view = WriteSetRecordView::new(prepared.record()).expect("the wire reads the record");
    assert_eq!(
        (view.namespace(), view.session(), view.request()),
        (NS, 1, 2)
    );
    assert_eq!(view.read_version(), prepared.read_version());
    let (executed, buffers) = prepared.into_parts();
    assert_eq!(executed.namespace(), NS);
    assert!(buffers.record_capacity() >= record_len);
    // The execution is what the commit step writes: applied as a round of one, it is held.
    let version = applier.quads().lists().next_version();
    applier.apply_round(vec![(version, executed)]).unwrap();
    assert_eq!(crate::utils::fixture::held(&applier, "a", "knows").len(), 1);
}

fn counter(delta: i64) -> ultragraphdb_store_apply::Operation {
    ultragraphdb_store_apply::Operation::CounterAdd {
        graph: oxrdf::GraphName::DefaultGraph,
        subject: crate::utils::fixture::ex("c").into(),
        predicate: crate::utils::fixture::ex("n"),
        delta,
    }
}

/// A transaction's operations ride in its proposal; once they run, the record is written again from the execution
/// with their writes, the session, request and commit record kept, and adopted as the proposal's (7.5).
#[test]
fn test_operations_run_on_top_of_the_transaction_and_its_record_is_written_again() {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadWrite);
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .unwrap();
    txn.operate(counter(4), 0).unwrap();
    let Ok(Commit::ReadWrite(mut prepared)) = txn.commit(3, 9, b"note", 0) else {
        panic!("a read-write commit");
    };
    assert!(prepared.has_operations());
    assert_eq!(prepared.operations(), &[counter(4)]);
    let before = prepared.record().to_vec();
    let overlay = applier.quads().lists().batch();
    let mut results = Vec::new();
    let operated = prepared
        .operated(&applier, &overlay, &[], &mut results)
        .unwrap();
    // A counter add's result is the counter's new value: 4 added to a counter holding nothing (point 33).
    assert_eq!(results, vec![(4, false)]);
    assert_eq!(
        prepared.record(),
        &before[..],
        "the proposal is left as it was"
    );
    let view = WriteSetRecordView::new(operated.record()).unwrap();
    assert_eq!((view.session(), view.request()), (3, 9));
    assert_eq!(view.commit_record(), b"note");
    assert!(
        view.list_deltas().len()
            > WriteSetRecordView::new(&before)
                .unwrap()
                .list_deltas()
                .len(),
        "the counter's list is written"
    );
    let record = operated.record().to_vec();
    prepared.adopt(operated);
    assert_eq!(prepared.record(), &record[..]);
    let (executed, _) = prepared.into_parts();
    let version = applier.quads().lists().applied_ts() + 2;
    applier.apply_round(vec![(version, executed)]).unwrap();
    assert_eq!(
        crate::utils::fixture::held(&applier, "c", "n"),
        vec![oxrdf::Term::from(oxrdf::Literal::from(4i64))]
    );
}

/// A transaction without operations keeps no copy of its commit record.
#[test]
fn test_a_transaction_without_operations_asks_none() {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadWrite);
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .unwrap();
    let Ok(Commit::ReadWrite(prepared)) = txn.commit(0, 0, b"note", 0) else {
        panic!("a read-write commit");
    };
    assert!(!prepared.has_operations());
    assert!(prepared.operations().is_empty());
}

/// A read-only transaction asks no operation; one past its limit is ended.
#[test]
fn test_an_operation_is_refused_in_a_read_only_or_ended_transaction() {
    let applier = applier();
    let mut read_only = begin(&applier, Mode::ReadOnly);
    assert_eq!(
        read_only.operate(counter(1), 0).unwrap_err(),
        ultragraphdb_transactions_txn::TxnError::ReadOnly()
    );
    let mut late = begin(&applier, Mode::ReadWrite);
    // 11 s past its begin, over the 10 s limit.
    let refused = late.operate(counter(1), 11_000_000).unwrap_err();
    assert_eq!(
        refused,
        ultragraphdb_transactions_txn::TxnError::PastLimit(10_000)
    );
    assert_eq!(
        late.operate(counter(1), 0).unwrap_err(),
        ultragraphdb_transactions_txn::TxnError::Ended()
    );
}

/// A build's end as a proposal: a sealed empty read set at the watermark and a record naming its operation, which ends
/// the build and starts none (decided 2026-10-28).
#[test]
fn test_a_builds_end_is_a_proposal_naming_its_operation() {
    use ultragraphdb_protocol_wire::RecordOperation;
    use ultragraphdb_store_apply::BuildEnd;
    use ultragraphdb_transactions_txn::{Prepared, TxnBuffers};
    let applier = applier();
    let prepared = Prepared::end_build(&applier, BuildEnd::Swap(8), TxnBuffers::new()).unwrap();
    assert_eq!(prepared.build_end(), Some(BuildEnd::Swap(8)));
    assert!(!prepared.starts_build() && !prepared.is_drop() && !prepared.has_operations());
    assert!(prepared.read_set().is_sealed());
    assert_eq!(
        prepared.read_set().read_version(),
        applier.quads().lists().applied_ts()
    );
    let record = WriteSetRecordView::new(prepared.record()).unwrap();
    assert_eq!(record.operation(), Some(RecordOperation::SwapBuild(8)));
    assert_eq!((record.session(), record.request()), (0, 0));
}

/// A transaction acting for a principal names it in its record, and in its record written again with its operations'
/// results.
#[test]
fn test_a_transaction_names_its_principal_in_its_record() {
    let applier = applier();
    let txn = begin(&applier, Mode::ReadWrite).with_principal(7);
    assert_eq!(txn.principal(), 7);
    let mut txn = txn;
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .unwrap();
    let Ok(Commit::ReadWrite(prepared)) = txn.commit(1, 2, &[], 0) else {
        panic!("a read-write commit");
    };
    assert_eq!(
        WriteSetRecordView::new(prepared.record())
            .unwrap()
            .principal(),
        7
    );
}

/// A drop through a session is a proposal naming its session, request and principal, its operation and its ranges, and
/// closes its round.
#[test]
fn test_a_drop_names_its_session_request_and_principal() {
    use ultragraphdb_protocol_wire::RecordOperation;
    use ultragraphdb_store_apply::DropRequest;
    use ultragraphdb_transactions_txn::{Prepared, TxnBuffers};
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadWrite);
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .unwrap();
    let Ok(Commit::ReadWrite(prepared)) = txn.commit(0, 0, &[], 0) else {
        panic!("a read-write commit");
    };
    let (executed, _) = prepared.into_parts();
    let version = applier.quads().lists().next_version();
    applier.apply_round(vec![(version, executed)]).unwrap();
    let drop =
        Prepared::drop(&applier, NS, DropRequest::Data, 11, 4, 7, TxnBuffers::new()).unwrap();
    assert!(drop.is_drop());
    assert!(drop.read_set().is_sealed());
    let view = WriteSetRecordView::new(drop.record()).unwrap();
    assert_eq!(
        (view.session(), view.request(), view.principal()),
        (11, 4, 7)
    );
    assert_eq!(view.operation(), Some(RecordOperation::DropNamespaceData));
    assert_eq!(
        view.conflict_ranges().len(),
        drop.executed().drop_ranges().len()
    );
}

/// A transaction's record written again with its operations' results keeps the principal it acted for.
#[test]
fn test_a_record_written_again_keeps_its_principal() {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadWrite).with_principal(7);
    txn.operate(counter(4), 0).unwrap();
    let Ok(Commit::ReadWrite(prepared)) = txn.commit(3, 9, b"note", 0) else {
        panic!("a read-write commit");
    };
    let overlay = applier.quads().lists().batch();
    let mut results = Vec::new();
    let operated = prepared
        .operated(&applier, &overlay, &[], &mut results)
        .unwrap();
    let mut prepared = prepared;
    prepared.adopt(operated);
    let view = WriteSetRecordView::new(prepared.record()).unwrap();
    assert_eq!((view.principal(), view.result_count()), (7, 1));
}
