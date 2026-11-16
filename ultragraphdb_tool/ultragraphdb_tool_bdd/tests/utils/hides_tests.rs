/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{TestResult, hides};

const SECRET: [u8; 4] = *b"key!";

/// Each form of a secret is found: as text, as its bytes compact or pretty, as hex; what holds none passes.
#[test]
fn test_hides_finds_every_form() -> TestResult {
    for printed in [
        "Holder { key: \"key!\" }".to_string(),
        format!("Holder {{ key: {SECRET:?} }}"),
        format!("Holder {{ key: {SECRET:#?} }}"),
        "Holder { key: 6b657921 }".to_string(),
    ] {
        assert!(hides(&printed, &SECRET).is_err(), "{printed}");
    }
    hides("Holder { key: <redacted> }", &SECRET)
}
