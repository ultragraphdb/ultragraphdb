/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use ultragraphdb_tool_bdd::{fuzz_regressions, fuzz_regressions_in, testdata_dir};

/// The cases committed under `testdata/fuzz_regressions/sample_target/`, as written there: `a_case` is the byte 0xFF,
/// `b_case` the bytes 0x01 0x02, `c_empty` no bytes; `nested/` is a directory and holds no case.
fn sample_cases() -> Vec<(String, Vec<u8>)> {
    vec![
        ("a_case".to_string(), vec![0xFF]),
        ("b_case".to_string(), vec![0x01, 0x02]),
        ("c_empty".to_string(), vec![]),
    ]
}

#[test]
fn test_a_target_s_cases_come_in_name_order_with_their_bytes() {
    assert_eq!(fuzz_regressions("sample_target"), sample_cases());
}

#[test]
fn test_a_target_without_regressions_has_none() {
    assert_eq!(fuzz_regressions("no_such_target"), Vec::new());
}

#[test]
fn test_a_directory_s_cases_are_read_from_it() {
    let dir = testdata_dir()
        .join("fuzz_regressions")
        .join("sample_target");
    assert_eq!(fuzz_regressions_in(&dir), sample_cases());
}

#[test]
fn test_a_path_that_is_a_file_holds_no_cases() {
    let file = testdata_dir()
        .join("fuzz_regressions")
        .join("sample_target")
        .join("a_case");
    assert_eq!(fuzz_regressions_in(&file), Vec::new());
}

#[test]
fn test_an_absent_directory_holds_no_cases() {
    assert_eq!(
        fuzz_regressions_in(&PathBuf::from("/no/such/fuzz/regressions")),
        Vec::new()
    );
}

/// A fresh directory under the system's temporary directory, named for this test and process.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ugdb_fuzz_regressions_{name}_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
#[should_panic(expected = "unreadable_case")]
fn test_an_unreadable_case_panics_naming_it() {
    let dir = scratch_dir("unreadable");
    let case = dir.join("unreadable_case");
    fs::write(&case, [0x00]).unwrap();
    fs::set_permissions(&case, fs::Permissions::from_mode(0o000)).unwrap();
    fuzz_regressions_in(&dir);
}
