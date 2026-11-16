/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_common_role::{RoleCluster, RoleSecret, retry_until, system_address};
use ultragraphdb_common_service::{RoleIdentity, RoleStart, Stopping};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::TransportError;
use ultragraphdb_tool_bdd::{Context, TestResult};

fn start(cluster: &str) -> TestResult<RoleStart> {
    let text = format!("[observability]\naddress = \"tcp://127.0.0.1:1\"\n[cluster]\n{cluster}");
    Ok(RoleStart::from_settings(
        text.as_bytes(),
        RoleIdentity::new("ci", "storage", 1),
    )?)
}

static FILES: AtomicU32 = AtomicU32::new(0);

/// A cluster whose deadline is `deadline`, its secret in a file no other test writes.
fn cluster(deadline: Duration) -> TestResult<RoleCluster> {
    let file = std::env::temp_dir().join(format!(
        "joining-{}-{}",
        std::process::id(),
        FILES.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&file, b"s3cret")?;
    let secret = RoleSecret::read(&file.display().to_string())?;
    Ok(RoleCluster::new(7, secret, deadline))
}

/// The settings of a start as `role` (`frontend`, `compute` or `storage`), `cluster` the lines of their `[cluster]`
/// table.
fn start_as(role: &str, cluster: &str) -> TestResult<RoleStart> {
    let text = format!("[observability]\naddress = \"tcp://127.0.0.1:1\"\n[cluster]\n{cluster}");
    Ok(RoleStart::from_settings(
        text.as_bytes(),
        RoleIdentity::new("ci", role, 1),
    )?)
}

/// C2.8: the system group's address is the one the settings name, on either carrier, for a frontend, a compute and a
/// storage instance alike.
#[test]
fn test_the_system_address_is_the_setting_for_every_role() -> TestResult {
    for role in ["frontend", "compute", "storage"] {
        for address in ["tcp://storage-0:7100", "unix:///run/ultragraph/system.sock"] {
            let start = start_as(role, &format!("system_address = \"{address}\"\n"))?;
            assert_eq!(system_address(&start)?.to_string(), address);
        }
    }
    Ok(())
}

/// The system group's address absent is refused as a configuration error naming `cluster.system_address`.
#[test]
fn test_an_absent_system_address_is_refused_naming_it() -> TestResult {
    let absent = system_address(&start("")?).err().context("a refusal")?;
    assert_eq!(absent.exit_code(), RoleError::EX_CONFIG);
    assert!(
        absent.to_string().contains("cluster.system_address"),
        "{absent}"
    );
    Ok(())
}

/// C2.7: a `cluster.system_address` that is no address is refused as a configuration error naming the setting and the
/// text it holds.
#[test]
fn test_a_system_address_that_is_no_address_is_refused_naming_it() -> TestResult {
    for text in [
        "storage-0:7100",
        "http://storage-0:7100",
        "tcp://storage-0",
        "unix://run/system.sock",
    ] {
        let refused = system_address(&start(&format!("system_address = \"{text}\"\n"))?)
            .err()
            .context("a refusal")?;
        assert_eq!(refused.exit_code(), RoleError::EX_CONFIG);
        let message = refused.to_string();
        assert!(message.contains("cluster.system_address"), "{message}");
        assert!(message.contains(text), "{message}");
    }
    Ok(())
}

/// C2.9, since the switch (design D2): settings naming `cluster.frontend_address` beside `cluster.system_address` are refused
/// at the start, naming the old setting, before any address is read.
#[test]
fn test_settings_naming_the_frontend_address_are_refused() -> TestResult {
    let refused = start(
        "frontend_address = \"tcp://frontend-0:7081\"\nsystem_address = \"unix:///run/ultragraph/system.sock\"\n",
    )
    .err()
    .context("a refusal")?;
    assert!(
        refused.to_string().contains("frontend_address"),
        "{refused}"
    );
    Ok(())
}

/// An attempt is made again until it succeeds within the deadline.
#[test]
fn test_a_join_is_tried_again_until_it_succeeds() -> TestResult {
    let tried = Cell::new(0);
    let joined = Runtime::new()?.block_on(retry_until(
        &cluster(Duration::from_secs(5))?,
        "storage",
        &Stopping::default(),
        || {
            tried.set(tried.get() + 1);
            let now = tried.get();
            async move {
                if now < 3 {
                    Err(TransportError::authentication("not yet"))
                } else {
                    Ok(now)
                }
            }
        },
    ))?;
    assert_eq!(joined, Some(3));
    Ok(())
}

/// A deadline past what the clock can name, as `join_deadline_ms` may state, never passes: the join goes on until it
/// succeeds, and nothing panics.
#[test]
fn test_a_deadline_past_the_clock_never_passes() -> TestResult {
    let tried = Cell::new(0);
    let joined = Runtime::new()?.block_on(retry_until(
        &cluster(Duration::MAX)?,
        "storage",
        &Stopping::default(),
        || {
            tried.set(tried.get() + 1);
            let now = tried.get();
            async move {
                if now < 2 {
                    Err(TransportError::authentication("not yet"))
                } else {
                    Ok(now)
                }
            }
        },
    ))?;
    assert_eq!(joined, Some(2));
    Ok(())
}

/// Past the deadline the last refusal ends the join, naming the attempts made; a stop ends it with nothing tried.
#[test]
fn test_a_join_ends_at_its_deadline_or_a_stop() -> TestResult {
    let tried = Cell::new(0u32);
    let refused = Runtime::new()?
        .block_on(retry_until(
            &cluster(Duration::from_millis(250))?,
            "storage",
            &Stopping::default(),
            || {
                tried.set(tried.get() + 1);
                async { Err::<(), _>(TransportError::authentication("refused")) }
            },
        ))
        .err()
        .context("a refusal past the deadline")?;
    assert_eq!(refused.exit_code(), RoleError::EX_UNAVAILABLE);
    let attempts = format!("after {} attempts", tried.get());
    assert!(
        refused.to_string().contains(&attempts),
        "{refused} {attempts}"
    );
    let stopping = Stopping::default();
    stopping.set();
    let stopped = Runtime::new()?.block_on(retry_until(
        &cluster(Duration::from_secs(5))?,
        "storage",
        &stopping,
        || async { Ok::<(), TransportError>(()) },
    ))?;
    assert_eq!(stopped, None);
    Ok(())
}
