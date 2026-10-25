/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_tool_bench::{BenchError, BenchErrorEnum};

#[test]
fn test_each_reason_says_what_failed() {
    assert_eq!(
        BenchError::Usage("unknown flag --x".to_string()).to_string(),
        "usage: unknown flag --x"
    );
    assert_eq!(
        BenchError::Io("no such dir".to_string()).to_string(),
        "i/o: no such dir"
    );
    let store = BenchError::from(StoreError::TsTooOld());
    assert!(matches!(store.0, BenchErrorEnum::Store(_)));
    assert_eq!(store.to_string(), "store: Transaction is too old");
}
