/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_common_role::{RoleCluster, RoleSecret, frontend_address, retry_until};
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

/// The frontend's address is the one the settings name; its absence names the setting.
#[test]
fn test_the_frontend_address_is_the_setting() -> TestResult {
    let named = frontend_address(&start("frontend_address = \"tcp://127.0.0.1:7081\"\n")?)?;
    assert_eq!(named.target(), "127.0.0.1:7081");
    let absent = frontend_address(&start("")?).err().context("a refusal")?;
    assert_eq!(absent.exit_code(), RoleError::EX_CONFIG);
    assert!(
        absent
            .to_string()
            .contains("the setting cluster.frontend_address is required"),
        "{absent}"
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
