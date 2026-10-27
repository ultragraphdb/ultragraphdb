/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::OperationResult;

#[test]
fn test_an_operation_result_keeps_its_uid_and_whether_it_created() {
    let result = OperationResult::new(42, true);
    assert_eq!(result.uid(), 42);
    assert!(result.created());
    assert!(!OperationResult::new(7, false).created());
    assert_eq!(OperationResult::new(7, false).uid(), 7);
}
