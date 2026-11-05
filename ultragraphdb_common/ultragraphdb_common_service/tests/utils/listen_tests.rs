/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use compio::runtime::Runtime;
use ultragraphdb_common_service::listen;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_tool_bdd::{Context, TestResult};

static PLACES: AtomicU32 = AtomicU32::new(0);

/// A directory of the test's own, short enough for a socket's path, which `std::env::temp_dir` is not on every host.
fn place() -> TestResult<PathBuf> {
    let at = PLACES.fetch_add(1, Ordering::Relaxed);
    let place = PathBuf::from(format!("/tmp/uglisten-{}-{at}", std::process::id()));
    std::fs::create_dir_all(&place)?;
    Ok(place)
}

/// What listening at `text` is refused with.
fn refused(text: &str) -> TestResult<RoleError> {
    let refusal = Runtime::new()?.block_on(listen(text)).err();
    refusal.context("a refusal")
}

/// A missing socket directory is the deployment's to create: the first thing an administrator meets says so.
#[test]
fn test_a_missing_socket_directory_says_to_create_it() -> TestResult {
    let directory = place()?.join("not-there");
    let error = refused(&format!("unix://{}/role.sock", directory.display()))?;
    assert_eq!(error.exit_code(), RoleError::EX_CONFIG, "{error}");
    let said = error.to_string();
    assert!(said.contains(&directory.display().to_string()), "{said}");
    assert!(said.contains("must exist and be writable"), "{said}");
    Ok(())
}

/// A file that is no socket is never removed for one: it may be what someone meant to keep.
#[test]
fn test_a_file_at_a_sockets_path_says_to_remove_it() -> TestResult {
    let path = place()?.join("role.sock");
    std::fs::write(&path, "kept")?;
    let error = refused(&format!("unix://{}", path.display()))?;
    assert_eq!(error.exit_code(), RoleError::EX_CONFIG, "{error}");
    let said = error.to_string();
    assert!(said.contains(&path.display().to_string()), "{said}");
    assert!(said.contains("is a file and no socket"), "{said}");
    assert_eq!(std::fs::read_to_string(&path)?, "kept");
    Ok(())
}

/// A port in use may free itself, so it is no configuration error.
#[test]
fn test_a_tcp_port_in_use_is_unavailable() -> TestResult {
    let held = std::net::TcpListener::bind("127.0.0.1:0")?;
    let error = refused(&format!("tcp://{}", held.local_addr()?))?;
    assert_eq!(error.exit_code(), RoleError::EX_UNAVAILABLE, "{error}");
    Ok(())
}
