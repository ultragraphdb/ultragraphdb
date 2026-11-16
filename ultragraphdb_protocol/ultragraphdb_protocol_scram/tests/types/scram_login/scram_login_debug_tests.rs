/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{ScramError, ScramLogin};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::hidden::hides;

const PASSWORD: &str = "a login's password that never prints";

fn nonce() -> Result<String, ScramError> {
    Ok("nonce".to_string())
}

/// A login's `Debug`, compact or pretty, holds its name and never its password.
#[test]
fn test_a_login_never_prints_its_password() -> TestResult {
    let nonces: fn() -> Result<String, ScramError> = nonce;
    let login = ScramLogin::new("user", PASSWORD, nonces)?;
    let printed = format!("{login:?}");
    for shown in [&printed, &format!("{login:#?}")] {
        hides(shown, PASSWORD.as_bytes())?;
    }
    assert!(printed.contains("name: \"user\""), "{printed}");
    Ok(())
}
