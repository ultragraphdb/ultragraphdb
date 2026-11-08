/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroUsize;
use std::time::Duration;

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transport_http_server::HttpLimits;

/// Limits take [`HttpLimits::CONNECTIONS_MAX`] connections at once unless told fewer, and telling them changes nothing else.
#[test]
fn test_limits_take_the_most_connections_unless_told_otherwise() -> TestResult {
    let limits = HttpLimits::new(64, Duration::from_secs(5));
    assert_eq!(limits.connections(), HttpLimits::CONNECTIONS_MAX);
    let fewer = NonZeroUsize::new(HttpLimits::CONNECTIONS_MAX.get() - 1).context("fewer")?;
    let told = limits.with_connections(fewer);
    assert_eq!(told.connections(), fewer);
    assert_eq!(
        (told.head(), told.body(), told.time()),
        (limits.head(), limits.body(), limits.time())
    );
    Ok(())
}
