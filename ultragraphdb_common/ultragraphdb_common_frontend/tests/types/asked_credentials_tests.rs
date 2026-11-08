/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_frontend::AskedCredentials;
use ultragraphdb_protocol_scram::{Credentials, ScramVerifier};

const NAME: &str = "ann";
const PRINCIPAL: u64 = 3;

fn verifier() -> ScramVerifier {
    ScramVerifier::new(vec![1, 2, 3], 4_096, [5; 32], [6; 32])
}

/// What the owner answered for a name is what the exchange looks up for it, once.
#[test]
fn test_the_owners_answer_is_looked_up_once_for_its_name() {
    let asked = AskedCredentials::default();
    asked.answered(NAME.to_string(), Some((PRINCIPAL, verifier())));
    assert_eq!(asked.lookup(NAME), Some((PRINCIPAL, verifier())));
    assert_eq!(asked.lookup(NAME), None, "taken by the first lookup");
}

/// An answer for one name gives nothing for another, nor does an owner answering that it holds no such name.
#[test]
fn test_an_answer_holds_for_its_name_alone() {
    let asked = AskedCredentials::default();
    asked.answered(NAME.to_string(), Some((PRINCIPAL, verifier())));
    assert_eq!(asked.lookup("bob"), None);
    asked.answered(NAME.to_string(), None);
    assert_eq!(asked.lookup(NAME), None);
}
