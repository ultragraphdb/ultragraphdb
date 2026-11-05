/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{Keyspace, QuadKey, QuadKeyErrorEnum, Region};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::quad_key::quad_key_tests::{every_keyspace, graph, id};

#[test]
fn test_a_region_is_the_keyspace_tenant_graph_and_predicate_of_its_key() -> TestResult {
    for key in every_keyspace()? {
        let region = Region::of(&key.to_bytes())?;
        assert_eq!(region, key.region());
        assert_eq!(
            (
                region.keyspace(),
                region.tenant(),
                region.graph(),
                region.predicate()
            ),
            (key.keyspace(), key.tenant(), key.graph(), key.predicate())
        );
    }
    Ok(())
}

#[test]
fn test_subject_predicate_keys_share_their_graphs_region() -> TestResult {
    let graph = ultragraphdb_core_keys::Graph::Named(id(1, 2, 2)?);
    let key = |subject| QuadKey::SubjectPredicates {
        tenant: 1,
        graph,
        subject,
    };
    let (a, b) = (key(id(1, 3, 3)?), key(id(1, 4, 4)?));
    assert_eq!(a.region(), b.region());
    assert_eq!(a.region().keyspace(), Keyspace::SubjectPredicates);
    assert_eq!(a.region().predicate(), None);
    Ok(())
}

#[test]
fn test_mark_and_referrer_regions_are_their_tenants() -> TestResult {
    let mark = |entity, group| QuadKey::Mark {
        tenant: 7,
        entity,
        group,
    };
    let (a, b) = (mark(id(1, 1, 1)?, 7), mark(id(2, 2, 2)?, 9));
    assert_eq!(a.region(), b.region());
    assert_eq!((a.region().graph(), a.region().predicate()), (None, None));
    let referrers = QuadKey::Referrers {
        tenant: 7,
        entity: id(1, 1, 1)?,
    };
    assert_ne!(
        a.region(),
        referrers.region(),
        "the keyspace tells them apart"
    );
    let catalog = QuadKey::Catalog {
        tenant: 7,
        kind: 1,
        id: id(1, 1, 1)?,
    };
    assert_eq!(
        (catalog.region().graph(), catalog.region().predicate()),
        (None, None)
    );
    assert_eq!(graph()?, graph()?);
    Ok(())
}

#[test]
fn test_the_region_of_a_key_that_does_not_parse_is_refused() {
    assert!(matches!(
        Region::of(&[0x99]).map_err(|e| e.0),
        Err(QuadKeyErrorEnum::UnknownKeyspace { .. })
    ));
}
