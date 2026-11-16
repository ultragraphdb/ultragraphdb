/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Challenge;
use ultragraphdb_tool_bdd::{Context, TestResult, hides};

use crate::types::challenge::challenge_tests::SALT;

/// A Challenge's `Debug`, request or reply, compact or pretty, holds its name, its iteration count and its salt's
/// length, and never the salt.
#[test]
fn test_a_challenge_prints_its_salts_length_never_the_salt() -> TestResult {
    let request = Challenge::request("user");
    let reply = request.clone().with_salt(4_096, SALT.to_vec());
    for challenge in [&request, &reply] {
        for shown in [format!("{challenge:?}"), format!("{challenge:#?}")] {
            hides(&shown, &SALT)?;
        }
        let printed = format!("{challenge:?}");
        for expected in [
            format!("name: {:?}", challenge.name()),
            format!("iterations: {}", challenge.iterations()),
            format!("salt: <{} bytes>", challenge.salt().len()),
        ] {
            printed
                .contains(&expected)
                .then_some(())
                .context(&format!("{printed} holds {expected}"))?;
        }
    }
    Ok(())
}
