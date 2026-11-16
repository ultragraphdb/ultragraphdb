/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataState};
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_tool_bdd::{TestResult, hides};

const SECRET: [u8; 32] = *b"the unknown names' secret, 32 B.";
const STORED_KEY: [u8; 32] = *b"the stored key, of 32 bytes, ok.";
const SERVER_KEY: [u8; 32] = *b"the server key, of 32 bytes, ok.";

/// A state's `Debug`, compact or pretty, says whether the secret is drawn and holds no form of it, nor a principal's
/// key; it holds the principal's name.
#[test]
fn test_a_state_never_prints_its_secret_or_a_key() -> TestResult {
    let mut state = MetadataState::new();
    assert!(format!("{state:?}").contains("unknown_secret: None"));
    state.apply(&MetadataCommand::DrawSecret { secret: SECRET })?;
    state.apply(&MetadataCommand::CreatePrincipal {
        id: 1,
        name: "ann".to_string(),
        verifier: ScramVerifier::new(vec![1, 2, 3], 4_096, STORED_KEY, SERVER_KEY),
        administrator: true,
    })?;
    let printed = format!("{state:?}");
    for shown in [&printed, &format!("{state:#?}")] {
        hides(shown, &SECRET)?;
        hides(shown, &STORED_KEY)?;
        hides(shown, &SERVER_KEY)?;
    }
    assert!(
        printed.contains("unknown_secret: Some(<redacted>)"),
        "{printed}"
    );
    assert!(printed.contains("\"ann\""), "{printed}");
    Ok(())
}
