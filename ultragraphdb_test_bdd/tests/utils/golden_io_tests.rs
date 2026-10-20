/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;

use ultragraphdb_test_bdd::{Golden, load_golden};

#[test]
fn a_golden_file_loads_with_its_commit_and_cases() {
    let path = std::env::temp_dir().join(format!("ugdb-golden-{}.json", std::process::id()));
    fs::write(
        &path,
        r#"{"dgraph_commit":"9fbf35614","generator":"t","cases":[1,2,3]}"#,
    )
    .unwrap();
    let g: Golden<u32> = load_golden(&path);
    assert_eq!(g.dgraph_commit(), "9fbf35614");
    assert_eq!(g.generator(), "t");
    assert_eq!(g.cases(), &[1, 2, 3]);
    let _ = fs::remove_file(path);
}
