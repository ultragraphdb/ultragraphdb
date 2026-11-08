/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The crash writer's round log as the harness reads it: the whole lines in order, none when it was never written, and a
//! log that does not read or holds a round out of order refused.

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::{acknowledged_rounds, fresh_dir};

/// A store directory of its own and the path of its round log beside it.
fn store_and_log(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = fresh_dir(name);
    let log = dir.with_extension("rounds");
    (dir, log)
}

#[test]
fn test_the_rounds_acknowledged_are_the_whole_lines_in_order() -> TestResult {
    let (dir, log) = store_and_log("rounds-whole");
    assert_eq!(acknowledged_rounds(&dir)?, 0, "no log, no round");
    // The third line was cut by the kill: no newline after it.
    std::fs::write(&log, "1\n2\n3").context("the log is written")?;
    assert_eq!(acknowledged_rounds(&dir)?, 2);
    std::fs::remove_file(&log).context("the log is removed")?;
    Ok(())
}

#[test]
fn test_a_round_logged_out_of_order_is_refused_naming_both() -> TestResult {
    let (dir, log) = store_and_log("rounds-order");
    let logged = [1u64, 3];
    let text: String = logged.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(&log, text).context("the log is written")?;
    let refused = acknowledged_rounds(&dir).err().context("refused")?;
    std::fs::remove_file(&log).context("the log is removed")?;
    // The second line names the round it holds, out of its place as the second.
    let second = logged.get(1).context("a second line")?;
    let named = format!("round {second} logged as round {}", logged.len());
    assert!(refused.to_string().contains(&named), "{refused}");
    Ok(())
}

#[test]
fn test_a_round_log_that_does_not_read_is_refused() -> TestResult {
    let (dir, log) = store_and_log("rounds-unreadable");
    // A directory where the log should be: it exists and does not read as text.
    std::fs::create_dir_all(&log).context("the directory is made")?;
    let refused = acknowledged_rounds(&dir);
    std::fs::remove_dir(&log).context("the directory is removed")?;
    assert!(refused.is_err(), "{refused:?}");
    Ok(())
}
