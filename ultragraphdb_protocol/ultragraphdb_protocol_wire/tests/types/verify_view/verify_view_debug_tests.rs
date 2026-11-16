/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyView, VerifyView};
use ultragraphdb_tool_bdd::{TestResult, hides};

use crate::types::verify::verify_tests::{AUTH_MESSAGE, PROOF, SIGNATURE};
use crate::utils::golden_bodies::frame_of;
use crate::utils::golden_roles::{golden_role, role_bytes};

/// A received Verify's `Debug`, request or verified reply, compact or pretty, holds its name, AuthMessage and principal
/// and never the ClientProof or the ServerSignature.
#[test]
fn test_a_verify_view_never_prints_its_proof_or_signature() -> TestResult {
    for name in ["verify", "verify_reply"] {
        let bytes = role_bytes(&golden_role(name)?)?;
        let view = VerifyView::from_frame(&frame_of(&bytes)?)?;
        for shown in [format!("{view:?}"), format!("{view:#?}")] {
            hides(&shown, &PROOF)?;
            hides(&shown, &SIGNATURE)?;
        }
        let printed = format!("{view:?}");
        assert!(printed.contains(AUTH_MESSAGE), "{printed}");
        assert!(
            printed.contains(&format!("principal: {}", view.principal())),
            "{printed}"
        );
    }
    Ok(())
}
