/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The names role connections authenticate under (`PROTOCOL.md` §11.12a: "the SCRAM usernames of role connections are
//! `compute` and `storage`"): law L6.5 of `openspec/changes/stage-5-9-compute-role/notes/laws.md`. What it expects is the
//! two names the section states, compared with the drawn name's bytes.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_role::RoleSecret;
use ultragraphdb_protocol_scram::Credentials;

/// The names the section states, and the retired role's.
const NAMED: [&str; 3] = ["compute", "storage", "frontend"];

/// A name one edit away from `name`: a byte dropped, replaced or added at a drawn place, drawn from the lower-case letters.
fn edited(tc: &TestCase, name: &str) -> String {
    let mut bytes = name.as_bytes().to_vec();
    let at = tc.draw(gs::integers::<usize>().min_value(0).max_value(bytes.len()));
    let letter = tc.draw(gs::integers::<u8>().min_value(b'a').max_value(b'z'));
    match tc.draw(gs::integers::<u8>().min_value(0).max_value(2)) {
        0 if at < bytes.len() => {
            let _removed: u8 = bytes.remove(at);
        }
        1 if at < bytes.len() => {
            if let Some(byte) = bytes.get_mut(at) {
                *byte = letter;
            }
        }
        _ => bytes.insert(at, letter),
    }
    String::from_utf8(bytes).unwrap_or_default()
}

/// L6.5: a SCRAM username of 1 to 16 bytes, drawn from the two role names, `frontend`, and names one edit away from each, is
/// known to a role's credentials exactly when it is `compute` or `storage`; every other name is unknown, which the exchange
/// refuses as it refuses a wrong secret (`ScramAuthenticator`, an unknown name's verifier derived from the server's secret).
#[hegel::test]
fn test_a_role_connection_authenticates_as_a_role_name_alone(tc: TestCase) {
    let file = std::env::temp_dir().join(format!("role-names-{}", std::process::id()));
    let credentials = std::fs::write(&file, "s3cret")
        .ok()
        .and_then(|()| RoleSecret::read(&file.display().to_string()).ok())
        .and_then(|secret| secret.credentials().ok());
    let base = tc.draw(gs::sampled_from(NAMED.to_vec()));
    let name = if tc.draw(gs::booleans()) {
        base.to_string()
    } else {
        edited(&tc, base)
    };
    let known = credentials.map(|held| held.lookup(&name).is_some());
    let expected = name == "compute" || name == "storage";
    assert_eq!(known, Some(expected), "{name}");
}
