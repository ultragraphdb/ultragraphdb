/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{AuthenticateView, BodyView};
use ultragraphdb_tool_bdd::{TestResult, hides};

use crate::utils::golden_bodies::{frame_of, golden};

/// Each received message of the golden exchange prints without the mechanism's message, compact or pretty, and with
/// its step.
#[test]
fn test_an_authenticate_view_never_prints_its_message() -> TestResult {
    for name in [
        "authenticate_client_first",
        "authenticate_server_first",
        "authenticate_client_final",
        "authenticate_server_final",
    ] {
        let bytes = golden(name)?.bytes()?;
        let view = AuthenticateView::from_frame(&frame_of(&bytes)?)?;
        for shown in [format!("{view:?}"), format!("{view:#?}")] {
            hides(&shown, view.message())?;
        }
        let printed = format!("{view:?}");
        assert!(
            printed.contains(&format!("step: {}", view.step())),
            "{printed}"
        );
    }
    Ok(())
}
