/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_compute::CoreListener;
use ultragraphdb_error_role::RoleErrorEnum;
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// A socket path `name` of this process in the test's temporary directory, short enough for `sockaddr_un`.
fn socket_path(name: &str) -> PathBuf {
    std::env::var_os("TEST_TMPDIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(format!("{}-{name}", std::process::id()))
}

/// Each core takes a handle of the listener as its own runtime's.
fn every_core_takes(bound: &CoreListener) -> TestResult {
    let another = bound.another()?;
    let _listener: Listener = Runtime::new()?.block_on(async { another.on_this_core() })?;
    Ok(())
}

/// A port a killed process holds a moment longer is taken once it lets go, as every other role's is.
#[test]
fn test_a_port_freed_within_the_bound_is_taken() -> TestResult {
    let held = TcpListener::bind("127.0.0.1:0")?;
    let text = format!("tcp://{}", held.local_addr()?);
    let bound = Runtime::new()?.block_on(async {
        spawn(async move {
            compio::time::sleep(Duration::from_millis(120)).await;
            drop(held);
        })
        .detach();
        CoreListener::bind(&text).await
    })?;
    assert!(matches!(bound, CoreListener::Tcp(_)), "{bound:?}");
    Ok(())
}

/// A TCP role address is one socket every core takes a handle of.
#[test]
fn test_every_core_takes_a_handle_of_a_tcp_listener() -> TestResult {
    // A role address names its port: one the system chose a moment ago, let go.
    let text = format!("tcp://{}", TcpListener::bind("127.0.0.1:0")?.local_addr()?);
    let bound = Runtime::new()?.block_on(CoreListener::bind(&text))?;
    assert!(matches!(bound, CoreListener::Tcp(_)), "{bound:?}");
    every_core_takes(&bound)
}

/// A Unix role address replaces the socket a process before this one left behind, opens to the user the roles run as
/// alone, and is one socket every core takes a handle of.
#[test]
fn test_a_unix_socket_replaces_a_stale_one_and_opens_to_its_user_alone() -> TestResult {
    let path = socket_path("stale.sock");
    drop(std::os::unix::net::UnixListener::bind(&path)?);
    let text = format!("unix://{}", path.display());
    let bound = Runtime::new()?.block_on(CoreListener::bind(&text))?;
    assert!(matches!(bound, CoreListener::Unix(_)), "{bound:?}");
    let mode = std::fs::metadata(&path)?.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "{mode:o}");
    every_core_takes(&bound)
}

/// A file at a Unix role address that is no socket is left standing, and the address refused as configuration naming it.
#[test]
fn test_a_file_standing_at_a_unix_address_is_refused() -> TestResult {
    let path = socket_path("file.sock");
    let content = b"no socket";
    std::fs::write(&path, content)?;
    let text = format!("unix://{}", path.display());
    let Err(refused) = Runtime::new()?.block_on(CoreListener::bind(&text)) else {
        return Err(TestError::new("a socket was bound over a file"));
    };
    assert!(
        matches!(&refused.0, RoleErrorEnum::Configuration { .. }),
        "{refused}"
    );
    assert!(refused.to_string().contains(&text), "{refused}");
    assert_eq!(std::fs::read(&path)?, content);
    Ok(())
}
