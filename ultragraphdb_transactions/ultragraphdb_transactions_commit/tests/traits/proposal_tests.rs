/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a proposal is by default: a Commit on an open transaction, with no token, epoch floor, dictionary entries or
//! operations, starting, ending and dropping nothing.

use ultragraphdb_transactions_commit::Proposal;
use ultragraphdb_transactions_txn::ReadSet;

/// A proposal stating its read set and record alone.
struct Bare {
    read_set: ReadSet,
}

impl Proposal for Bare {
    fn read_set(&self) -> &ReadSet {
        &self.read_set
    }

    fn record(&self) -> &[u8] {
        &[]
    }
}

#[test]
fn test_a_proposal_is_by_default_a_commit_on_an_open_transaction_asking_nothing_more() {
    let bare = Bare {
        read_set: ReadSet::new(),
    };
    assert!(!bare.is_one_frame(), "a Commit on an open transaction");
    assert!(bare.token().is_none());
    assert!(bare.epoch_floor().is_none());
    assert_eq!(bare.dictionary().count(), 0);
    assert!(!bare.has_operations());
    assert!(!bare.starts_build());
    assert!(!bare.ends_build());
    assert!(!bare.is_drop());
}
