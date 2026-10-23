/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_telemetry::{TelemetryError, TelemetryErrorEnum};

#[test]
fn test_constructors_build_their_variant() {
    assert_eq!(
        TelemetryError::Filter("x=loud".into(), "bad".into()).0,
        TelemetryErrorEnum::Filter {
            directive: "x=loud".into(),
            reason: "bad".into()
        }
    );
    assert_eq!(TelemetryError::Runtime().0, TelemetryErrorEnum::Runtime);
    assert_eq!(
        TelemetryError::Exporter("e".into(), "bad".into()).0,
        TelemetryErrorEnum::Exporter {
            endpoint: "e".into(),
            reason: "bad".into()
        }
    );
    assert_eq!(
        TelemetryError::Installed("set".into()).0,
        TelemetryErrorEnum::Installed("set".into())
    );
}
