/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::{Credential, MechanismStep};
use ultragraphdb_tool_bdd::{TestResult, hides};

/// RFC 7677's server-final-message, which carries the ServerSignature.
const SERVER_FINAL: &[u8] = b"v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=";

/// A step's `Debug`, compact or pretty, holds no message and holds the credential a final step established.
#[test]
fn test_a_step_never_prints_its_message() -> TestResult {
    let credential = Credential::new(42);
    for step in [
        MechanismStep::Continue(SERVER_FINAL.to_vec()),
        MechanismStep::Done(SERVER_FINAL.to_vec(), credential),
    ] {
        for shown in [format!("{step:?}"), format!("{step:#?}")] {
            hides(&shown, SERVER_FINAL)?;
        }
    }
    let done = format!(
        "{:?}",
        MechanismStep::Done(SERVER_FINAL.to_vec(), credential)
    );
    assert!(done.contains(&format!("{credential:?}")), "{done}");
    Ok(())
}
