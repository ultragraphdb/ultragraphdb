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

#[test]
fn test_a_counters_new_value_is_the_uid_field_in_twos_complement() {
    for value in [0i64, 1, 7, -1, -5, i64::MIN, i64::MAX] {
        let result = OperationResult::counter(value);
        assert_eq!(result.counter_value(), value);
        assert_eq!(result.uid(), value as u64);
        assert!(!result.created());
    }
    assert_eq!(OperationResult::counter(-1).uid(), u64::MAX);
    assert_eq!(OperationResult::new(u64::MAX, false).counter_value(), -1);
}
