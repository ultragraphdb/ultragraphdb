/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_commit::CommitRequest;

#[test]
fn test_a_request_keeps_its_ticket_and_proposal() {
    let request = CommitRequest::new(7, "p");
    assert_eq!(request.ticket(), 7);
    assert_eq!(*request.proposal(), "p");
    assert_eq!(request.into_parts(), (7, "p"));
}
