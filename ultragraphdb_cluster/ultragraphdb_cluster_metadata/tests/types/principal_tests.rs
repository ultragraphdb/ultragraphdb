/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::Principal;
use ultragraphdb_protocol_scram::ScramVerifier;

#[test]
fn test_a_principal_holds_its_fields() {
    let verifier = ScramVerifier::new(vec![1, 2], 4_096, [3; 32], [4; 32]);
    let principal = Principal::new(5, "ann".to_string(), verifier.clone(), true);
    assert_eq!(principal.id(), 5);
    assert_eq!(principal.name(), "ann");
    assert_eq!(principal.verifier(), &verifier);
    assert!(principal.administrator());
    assert!(!Principal::new(6, "bob".to_string(), verifier, false).administrator());
}
