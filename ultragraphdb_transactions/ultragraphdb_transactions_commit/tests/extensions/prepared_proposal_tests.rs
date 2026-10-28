/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_commit::Proposal;

use crate::utils::fixture::{applier, begin, insert, prepare, read, setup, write};

#[test]
fn test_a_prepared_transaction_proposes_its_read_set_and_record() {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let mut txn = begin(&applier);
    read(&mut txn, "a", "knows");
    write(&mut txn, vec![insert("a", "knows", "c")]);
    let prepared = prepare(txn);
    assert!(std::ptr::eq(
        Proposal::read_set(&prepared),
        prepared.read_set()
    ));
    assert_eq!(Proposal::record(&prepared), prepared.record());
    assert!(std::ptr::eq(
        Proposal::record(&prepared).as_ptr(),
        prepared.record().as_ptr()
    ));
    assert!(Proposal::read_set(&prepared).is_sealed());
    assert!(Proposal::read_set(&prepared).lists().count() > 0);
}
