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
