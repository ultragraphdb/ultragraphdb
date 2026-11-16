/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{TestResult, hides};

use crate::types::authenticate::authenticate_tests::golden_exchange;

/// Each message of the golden exchange, the client's final with its proof and the server's final with its signature
/// among them, prints without the mechanism's message, compact or pretty, and with its step and mechanism.
#[test]
fn test_an_authenticate_never_prints_its_message() -> TestResult {
    for message in golden_exchange() {
        for shown in [format!("{message:?}"), format!("{message:#?}")] {
            hides(&shown, message.message())?;
        }
        let printed = format!("{message:?}");
        assert!(
            printed.contains(&format!("step: {}", message.step())),
            "{printed}"
        );
        assert!(
            printed.contains(&format!("mechanism: {:?}", message.mechanism())),
            "{printed}"
        );
    }
    Ok(())
}
