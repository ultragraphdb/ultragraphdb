/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyView, ChallengeView};
use ultragraphdb_tool_bdd::{Context, TestResult, hides};

use crate::types::challenge::challenge_tests::SALT;
use crate::utils::golden_bodies::frame_of;
use crate::utils::golden_roles::{golden_role, role_bytes};

/// A received Challenge's `Debug`, request or reply, compact or pretty, holds its name, its iteration count and its
/// salt's length, and never the salt.
#[test]
fn test_a_challenge_view_prints_its_salts_length_never_the_salt() -> TestResult {
    for name in ["challenge", "challenge_reply"] {
        let bytes = role_bytes(&golden_role(name)?)?;
        let view = ChallengeView::from_frame(&frame_of(&bytes)?)?;
        for shown in [format!("{view:?}"), format!("{view:#?}")] {
            hides(&shown, &SALT)?;
        }
        let printed = format!("{view:?}");
        for expected in [
            format!("name: {:?}", view.name()),
            format!("iterations: {}", view.iterations()),
            format!("salt: <{} bytes>", view.salt().len()),
        ] {
            printed
                .contains(&expected)
                .then_some(())
                .context(&format!("{printed} holds {expected}"))?;
        }
    }
    Ok(())
}
