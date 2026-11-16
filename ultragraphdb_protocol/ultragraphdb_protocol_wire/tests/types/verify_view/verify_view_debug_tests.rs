/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyView, VerifyView};
use ultragraphdb_tool_bdd::{Context, TestResult, hides};

use crate::types::verify::verify_tests::{PROOF, SIGNATURE};
use crate::utils::golden_bodies::frame_of;
use crate::utils::golden_roles::{golden_role, role_bytes};

/// A received Verify's `Debug`, request or verified reply, compact or pretty, holds its name, its AuthMessage's length
/// and its principal, and never the AuthMessage, the ClientProof or the ServerSignature.
#[test]
fn test_a_verify_view_never_prints_its_proof_or_signature() -> TestResult {
    for name in ["verify", "verify_reply"] {
        let bytes = role_bytes(&golden_role(name)?)?;
        let view = VerifyView::from_frame(&frame_of(&bytes)?)?;
        for shown in [format!("{view:?}"), format!("{view:#?}")] {
            hides(&shown, &PROOF)?;
            hides(&shown, &SIGNATURE)?;
            hides(&shown, view.auth_message().as_bytes())?;
        }
        let printed = format!("{view:?}");
        let length = format!("auth_message: <{} bytes>", view.auth_message().len());
        printed
            .contains(&length)
            .then_some(())
            .context(&format!("{printed} holds {length}"))?;
        assert!(
            printed.contains(&format!("principal: {}", view.principal())),
            "{printed}"
        );
    }
    Ok(())
}
