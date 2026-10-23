/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{ReservedNamespace, TrustMarker};

#[test]
fn test_builder_keeps_every_claim() {
    let marker = TrustMarker::new("builder");
    let ns = ReservedNamespace::new()
        .with_predicate_prefix("dgraph.b.rel.")
        .with_predicates(["dgraph.b.p"])
        .with_types(["dgraph.b.T"])
        .with_value_locked(["dgraph.b.p"])
        .with_value_locked_prefixes(["dgraph.b.rel."])
        .with_trust_marker(marker);
    assert_eq!(ns.predicate_prefix(), "dgraph.b.rel.");
    assert_eq!(ns.predicates(), ["dgraph.b.p"]);
    assert_eq!(ns.types(), ["dgraph.b.T"]);
    assert_eq!(ns.value_locked(), ["dgraph.b.p"]);
    assert_eq!(ns.value_locked_prefixes(), ["dgraph.b.rel."]);
    assert_eq!(ns.trust_marker(), Some(marker));
}

#[test]
fn test_default_claims_nothing() {
    let ns = ReservedNamespace::default();
    assert_eq!(ns, ReservedNamespace::new());
    assert!(
        ns.predicate_prefix().is_empty() && ns.predicates().is_empty() && ns.types().is_empty()
    );
    assert!(ns.value_locked().is_empty() && ns.value_locked_prefixes().is_empty());
    assert_eq!(ns.trust_marker(), None);
}
