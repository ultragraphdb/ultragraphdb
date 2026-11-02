/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use ultragraphdb_tool_bdd::{
    TestError, TestResult, fuzz_regressions, fuzz_regressions_in, testdata_dir,
};

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
fn test_a_target_s_cases_come_in_name_order_with_their_bytes() -> TestResult {
    assert_eq!(fuzz_regressions("sample_target")?, sample_cases());
    Ok(())
}

#[test]
fn test_a_target_without_regressions_has_none() -> TestResult {
    assert_eq!(fuzz_regressions("no_such_target")?, Vec::new());
    Ok(())
}

#[test]
fn test_a_directory_s_cases_are_read_from_it() -> TestResult {
    let dir = testdata_dir()?
        .join("fuzz_regressions")
        .join("sample_target");
    assert_eq!(fuzz_regressions_in(&dir)?, sample_cases());
    Ok(())
}

#[test]
fn test_a_path_that_is_a_file_holds_no_cases() -> TestResult {
    let file = testdata_dir()?
        .join("fuzz_regressions")
        .join("sample_target")
        .join("a_case");
    assert_eq!(fuzz_regressions_in(&file)?, Vec::new());
    Ok(())
}

#[test]
fn test_an_absent_directory_holds_no_cases() -> TestResult {
    assert_eq!(
        fuzz_regressions_in(&PathBuf::from("/no/such/fuzz/regressions"))?,
        Vec::new()
    );
    Ok(())
}

/// A fresh directory under the system's temporary directory, named for this test and process.
fn scratch_dir(name: &str) -> TestResult<PathBuf> {
    let dir = std::env::temp_dir().join(format!(
        "ugdb_fuzz_regressions_{name}_{}",
        std::process::id()
    ));
    // A previous run's directory is cleared; there is none on a first run.
    if let Err(e) = fs::remove_dir_all(&dir) {
        assert_eq!(
            e.kind(),
            std::io::ErrorKind::NotFound,
            "{}: {e}",
            dir.display()
        );
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[test]
fn test_an_unreadable_case_fails_naming_it() -> TestResult {
    let dir = scratch_dir("unreadable")?;
    let case = dir.join("unreadable_case");
    fs::write(&case, [0x00])?;
    fs::set_permissions(&case, fs::Permissions::from_mode(0o000))?;
    let Err(e) = fuzz_regressions_in(&dir) else {
        return Err(TestError::new("an unreadable case fails the replay"));
    };
    assert!(e.reason().contains("unreadable_case"), "{e}");
    Ok(())
}
