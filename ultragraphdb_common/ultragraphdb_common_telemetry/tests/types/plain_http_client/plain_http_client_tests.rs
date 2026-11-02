/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_common_telemetry::PlainHttpClient;

#[test]
fn test_new_keeps_its_timeout() {
    let client = PlainHttpClient::new(Duration::from_millis(250));
    assert_eq!(client.timeout(), Duration::from_millis(250));
}
