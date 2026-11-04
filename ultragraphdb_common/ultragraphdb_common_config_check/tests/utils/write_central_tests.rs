/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use ultragraphdb_common_config_check::{CENTRAL_FILE, TargetFiles, read_central, write_central};
use ultragraphdb_tool_bdd::{Context, TestResult};

static FOLDERS: AtomicU32 = AtomicU32::new(0);

/// A folder no other test names, not yet made.
fn folder() -> PathBuf {
    std::env::var_os("TEST_TMPDIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(format!(
            "write-central-{}-{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::Relaxed)
        ))
}

fn target(name: &str, key: &str) -> TargetFiles {
    let mut files = TargetFiles::new(name);
    files.insert(key, key, b"[storage]\npartition_count = 1\n\n".to_vec());
    files
}

/// What is written reads back byte for byte, and names that flat files would collide on stay apart.
#[test]
fn test_a_folder_written_reads_back() -> TestResult {
    let at = folder();
    let written = [
        target("a_storage", "storage"),
        target("a", "storage_storage"),
    ];
    let central = write_central(&at, &written)?;
    assert_eq!(central, at.join(CENTRAL_FILE));
    let read = read_central(&central)
        .map_err(|defects| ultragraphdb_tool_bdd::TestError::new(format!("{defects:?}")))?;
    for files in &written {
        let held = read
            .iter()
            .find(|target| target.name() == files.name())
            .context("the target")?;
        assert!(held.same_files(files), "{}", files.name());
    }
    Ok(())
}

/// Names come from a peer: none reaches the disk unchecked, and nothing is written for one that fails.
#[test]
fn test_a_name_that_cannot_name_a_file_writes_nothing() -> TestResult {
    for (bad, says) in [
        (target("../escape", "storage"), "cannot name a folder"),
        (target("", "storage"), "cannot name a folder"),
        (target("ci", "../../storage"), "names no role"),
        (target("ci", "other"), "names no role"),
    ] {
        let at = folder();
        let refused = write_central(&at, &[bad]).err().context("a refusal")?;
        assert!(refused.to_string().contains(says), "{refused}");
        assert!(!at.exists(), "nothing was written");
    }
    Ok(())
}

/// A folder holding anything is left as it was; an empty one is taken.
#[test]
fn test_a_folder_holding_files_is_refused() -> TestResult {
    let at = folder();
    std::fs::create_dir_all(&at)?;
    assert!(
        write_central(&at, &[target("ci", "storage")]).is_ok(),
        "an empty folder"
    );
    let refused = write_central(&at, &[target("ci", "storage")])
        .err()
        .context("a refusal")?;
    assert!(
        refused.to_string().contains("already holds files"),
        "{refused}"
    );
    Ok(())
}
