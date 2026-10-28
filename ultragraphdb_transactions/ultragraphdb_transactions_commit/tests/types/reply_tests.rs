/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_commit::{CommitError, Reply};

#[test]
fn test_a_reply_keeps_its_ticket_and_outcome() {
    let committed = Reply::new(3, Ok(12));
    assert_eq!(committed.ticket(), 3);
    assert_eq!(committed.outcome(), &Ok(12));
    let refused = Reply::new(4, Err(CommitError::Unavailable()));
    assert_eq!(refused.outcome(), &Err(CommitError::Unavailable()));
}
