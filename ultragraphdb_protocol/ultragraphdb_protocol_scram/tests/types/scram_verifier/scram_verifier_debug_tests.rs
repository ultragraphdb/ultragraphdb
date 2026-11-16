/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;

use crate::utils::hidden::hides;
use crate::utils::rfc7677::verifier;

/// A verifier's `Debug`, compact or pretty, holds neither key; it holds the salt and the iteration count.
#[test]
fn test_a_verifier_never_prints_its_keys() -> TestResult {
    let verifier = verifier()?;
    let printed = format!("{verifier:?}");
    for shown in [&printed, &format!("{verifier:#?}")] {
        hides(shown, verifier.stored_key())?;
        hides(shown, verifier.server_key())?;
    }
    assert!(
        printed.contains(&format!("salt: {:?}", verifier.salt())),
        "{printed}"
    );
    assert!(
        printed.contains(&format!("iterations: {}", verifier.iterations())),
        "{printed}"
    );
    Ok(())
}
