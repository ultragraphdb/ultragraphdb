/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Whether a value's `Debug` hides a secret, in the forms SCRAM adds.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// `Ok` when `printed` holds none of the forms [`ultragraphdb_tool_bdd::hides`] looks for, nor `secret`'s base64
/// without its padding, as a SCRAM message carries it.
pub fn hides(printed: &str, secret: &[u8]) -> TestResult {
    ultragraphdb_tool_bdd::hides(printed, secret)?;
    let base64 = STANDARD.encode(secret);
    let unpadded = base64.trim_end_matches('=');
    if printed.contains(unpadded) {
        return Err(TestError::new(format!("{printed} holds {unpadded}")));
    }
    Ok(())
}
