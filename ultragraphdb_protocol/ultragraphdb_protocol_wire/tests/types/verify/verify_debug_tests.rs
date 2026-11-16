/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Verify;
use ultragraphdb_tool_bdd::{Context, TestResult, hides};

use crate::types::verify::verify_tests::{AUTH_MESSAGE, PROOF, SIGNATURE};

/// A Verify's `Debug`, request or verified reply, compact or pretty, holds its name, its AuthMessage's length and its
/// principal, and never the AuthMessage, the ClientProof or the ServerSignature.
#[test]
fn test_a_verify_never_prints_its_proof_or_signature() -> TestResult {
    let request = Verify::request("user", AUTH_MESSAGE, PROOF);
    let reply = request.clone().with_verified(7, SIGNATURE);
    for verify in [&request, &reply] {
        for shown in [format!("{verify:?}"), format!("{verify:#?}")] {
            hides(&shown, &PROOF)?;
            hides(&shown, &SIGNATURE)?;
            hides(&shown, verify.auth_message().as_bytes())?;
        }
        let printed = format!("{verify:?}");
        let length = format!("auth_message: <{} bytes>", verify.auth_message().len());
        printed
            .contains(&length)
            .then_some(())
            .context(&format!("{printed} holds {length}"))?;
    }
    let printed = format!("{reply:?}");
    assert!(printed.contains("verified: Some((7, "), "{printed}");
    Ok(())
}
