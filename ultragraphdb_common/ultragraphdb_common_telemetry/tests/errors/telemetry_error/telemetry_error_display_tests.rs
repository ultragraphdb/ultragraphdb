/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_telemetry::TelemetryError;

#[test]
fn test_display_names_the_input_and_the_reason() {
    assert_eq!(
        TelemetryError::filter("x=loud".into(), "invalid level".into()).to_string(),
        "invalid log filter \"x=loud\": invalid level"
    );
    assert_eq!(
        TelemetryError::exporter("e".into(), "invalid uri".into()).to_string(),
        "invalid OTLP endpoint \"e\": invalid uri"
    );
    assert_eq!(
        TelemetryError::installed("a global default is set".into()).to_string(),
        "telemetry already installed: a global default is set"
    );
}
