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
    assert!(WriteSetRecordView::new(prepared.record()).is_ok());
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
    assert_eq!(results, vec![(0, false)]);
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
