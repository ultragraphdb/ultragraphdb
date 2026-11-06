/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::TcpListener;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_compute::CoreListener;
use ultragraphdb_tool_bdd::TestResult;

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
