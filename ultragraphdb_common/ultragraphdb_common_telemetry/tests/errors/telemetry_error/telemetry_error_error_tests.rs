/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_common_telemetry::TelemetryError;

#[test]
fn test_is_a_std_error_without_a_source() {
    assert!(TelemetryError::Runtime().source().is_none());
}
