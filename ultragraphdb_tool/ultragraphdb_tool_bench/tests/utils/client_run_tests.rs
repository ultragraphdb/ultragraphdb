/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::TcpListener;
use std::path::PathBuf;

use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_bench::{BenchErrorEnum, ClientArgs, run_client};

/// A file of the test's own holding a password.
fn password_file(name: &str) -> TestResult<PathBuf> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let file = root.join(format!("bench-client-{name}-{}", std::process::id()));
    std::fs::write(&file, "secret")?;
    Ok(file)
}

fn args(address: &str, password: &std::path::Path) -> TestResult<ClientArgs> {
    let owned: Vec<String> = [
        "--address",
        address,
        "--op",
        "direct",
        "--seconds",
        "1",
        "--password-file",
    ]
    .iter()
    .map(ToString::to_string)
    .chain([password.display().to_string()])
    .collect();
    Ok(ClientArgs::parse(&owned)?)
}

/// A run whose endpoint no process listens on stops at its first connection, naming the address.
#[test]
fn test_a_run_against_no_endpoint_names_the_address() -> TestResult {
    let address = {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.local_addr()?.to_string()
    };
    let refused = run_client(&args(&address, &password_file("closed")?)?);
    let Err(error) = refused else {
        return Err(TestError::new(format!(
            "a run against nothing measured: {refused:?}"
        )));
    };
    assert!(
        matches!(&error.0, BenchErrorEnum::Client(reason) if reason.starts_with(&address)),
        "{error}"
    );
    assert!(error.to_string().starts_with("client: "), "{error}");
    Ok(())
}

/// A run whose password file cannot be read stops before it dials, naming the file.
#[test]
fn test_an_unreadable_password_file_is_named() -> TestResult {
    let missing = password_file("missing")?.with_extension("absent");
    let refused = run_client(&args("127.0.0.1:9", &missing)?);
    let error = refused.err().context("a run without its password")?;
    assert!(
        matches!(&error.0, BenchErrorEnum::Io(reason) if reason.contains("absent")),
        "{error}"
    );
    Ok(())
}
