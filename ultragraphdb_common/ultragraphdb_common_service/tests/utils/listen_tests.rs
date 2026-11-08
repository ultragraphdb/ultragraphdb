/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_service::{BIND_RETRIES, listen};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

static PLACES: AtomicU32 = AtomicU32::new(0);

/// A directory of the test's own, short enough for a socket's path, which `std::env::temp_dir` is not on every host.
fn place() -> TestResult<PathBuf> {
    let at = PLACES.fetch_add(1, Ordering::Relaxed);
    let place = PathBuf::from(format!("/tmp/uglisten-{}-{at}", std::process::id()));
    std::fs::create_dir_all(&place)?;
    Ok(place)
}

/// How long a listen may take before the test fails: past the 5 s of retries a port in use is given.
const LISTEN_WAIT: Duration = Duration::from_secs(8);

/// What listening at `text` is refused with, within [`LISTEN_WAIT`].
fn refused(text: &str) -> TestResult<RoleError> {
    let refusal = Runtime::new()?
        .block_on(compio::time::timeout(LISTEN_WAIT, listen(text)))
        .map_err(|_| TestError::new(format!("listening at {text} within {LISTEN_WAIT:?}")))?
        .err();
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

/// A port in use may free itself, so it is no configuration error: past its retries the refusal names the port and what to do.
#[test]
fn test_a_tcp_port_in_use_is_unavailable() -> TestResult {
    let held = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = held.local_addr()?;
    let error = refused(&format!("tcp://{address}"))?;
    assert_eq!(error.exit_code(), RoleError::EX_UNAVAILABLE, "{error}");
    let said = error.to_string();
    assert!(said.contains(&address.to_string()), "{said}");
    assert!(said.contains(&BIND_RETRIES.to_string()), "{said}");
    assert!(said.contains("stop it"), "{said}");
    Ok(())
}

/// A port a killed process holds a moment longer is taken once it lets go.
#[test]
fn test_a_port_freed_within_the_bound_is_taken() -> TestResult {
    let held = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = held.local_addr()?;
    let listener = Runtime::new()?.block_on(async {
        spawn(async move {
            compio::time::sleep(Duration::from_millis(120)).await;
            drop(held);
        })
        .detach();
        listen(&format!("tcp://{address}")).await
    })?;
    assert_eq!(listener.local_address()?.target(), address.to_string());
    Ok(())
}

/// What no wait can mend ends the start at once, naming the address: no interface of this host holds this one (RFC 5737).
#[test]
fn test_a_bind_error_other_than_a_port_in_use_is_unavailable() -> TestResult {
    let address = "203.0.113.1:18099";
    let error = refused(&format!("tcp://{address}"))?;
    assert_eq!(error.exit_code(), RoleError::EX_UNAVAILABLE, "{error}");
    let said = error.to_string();
    assert!(said.contains(address), "{said}");
    assert!(!said.contains("retries"), "{said}");
    Ok(())
}

/// A Unix socket is made at mode 0600, and the socket a process before this one left behind is replaced.
#[test]
fn test_a_unix_socket_is_private_and_a_stale_one_replaced() -> TestResult {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    let path = place()?.join("role.sock");
    let text = format!("unix://{}", path.display());
    let runtime = Runtime::new()?;
    for _ in 0..2 {
        // The first listen makes the socket; the second finds it left behind by the first, dropped.
        let listener = runtime
            .block_on(compio::time::timeout(LISTEN_WAIT, listen(&text)))
            .map_err(|_| TestError::new(format!("listening at {text} within {LISTEN_WAIT:?}")))??;
        let made = std::fs::symlink_metadata(&path)?;
        assert!(made.file_type().is_socket());
        assert_eq!(made.permissions().mode() & 0o777, 0o600);
        drop(listener);
    }
    Ok(())
}
