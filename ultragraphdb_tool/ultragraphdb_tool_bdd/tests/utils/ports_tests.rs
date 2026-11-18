/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::env;
use std::process::{Command, Output};

use ultragraphdb_tool_bdd::{
    Context, PORT_BASE, PORT_BLOCK, TestResult, loopback, port, port_from,
};

/// Set in a child run of this binary to make `child_reports_the_port` print what it finds. Where the base is refused, the
/// child fails, and libtest prints the returned error to stderr as `Error: <its Debug>`.
const CHILD: &str = "UGDB_BDD_PORTS_CHILD";

#[test]
fn child_reports_the_port() -> TestResult {
    if env::var_os(CHILD).is_some() {
        println!("port={}", port(7)?);
        println!("loopback={}", loopback(7)?);
    }
    Ok(())
}

/// Runs `child_reports_the_port` in a fresh process, its base `base` or none.
fn run_child(base: Option<&str>) -> TestResult<Output> {
    let mut command = Command::new(env::current_exe()?);
    command
        .args([
            "utils::ports_tests::child_reports_the_port",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .env_remove(PORT_BASE);
    if let Some(base) = base {
        command.env(PORT_BASE, base);
    }
    Ok(command.output()?)
}

#[test]
fn a_port_is_the_base_plus_its_offset() -> TestResult {
    assert_eq!(port_from("20000", 0)?, 20_000);
    assert_eq!(port_from("20000", PORT_BLOCK - 1)?, 20_000 + PORT_BLOCK - 1);
    Ok(())
}

#[test]
fn an_offset_past_the_block_is_refused() -> TestResult {
    let refused = port_from("20000", PORT_BLOCK)
        .err()
        .context("an offset past the block")?;
    assert!(
        refused.to_string().contains("past a target's block"),
        "{refused}"
    );
    Ok(())
}

#[test]
fn a_base_that_is_no_port_is_refused_by_name() -> TestResult {
    for base in ["", "seventeen", "70000", "-1"] {
        let refused = port_from(base, 0).err().context("a base that is no port")?;
        assert!(refused.to_string().contains(PORT_BASE), "{refused}");
    }
    let refused = port_from("65500", 100)
        .err()
        .context("a sum past the last port")?;
    assert!(
        refused.to_string().contains("passes the last port"),
        "{refused}"
    );
    Ok(())
}

#[test]
fn the_port_is_read_from_the_variable() -> TestResult {
    let out = run_child(Some("21000"))?;
    let stdout = String::from_utf8(out.stdout)?;
    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("port=21007"), "{stdout}");
    assert!(stdout.contains("loopback=127.0.0.1:21007"), "{stdout}");
    Ok(())
}

#[test]
fn an_unset_variable_fails_naming_it() -> TestResult {
    let out = run_child(None)?;
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr)?;
    assert!(
        stderr.contains(&format!("{PORT_BASE} is not set")),
        "{stderr}"
    );
    Ok(())
}
