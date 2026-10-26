/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{Keyspace, QuadKey, Region};

use crate::types::quad_key::quad_key_tests::every_keyspace;

#[test]
fn test_a_region_is_the_keyspace_namespace_graph_and_predicate_of_its_key() {
    for key in every_keyspace() {
        let region = Region::of(&key.to_bytes()).unwrap();
        assert_eq!(region, key.region());
        assert_eq!(
            (
                region.keyspace(),
                region.namespace(),
                region.graph(),
                region.predicate()
            ),
            (
                key.keyspace(),
                key.namespace(),
                key.graph(),
                key.predicate()
            )
        );
    }
    let a = QuadKey::SubjectPredicates {
        namespace: 1,
        graph: 2,
        subject: 3,
    };
    let b = QuadKey::SubjectPredicates {
        namespace: 1,
        graph: 2,
        subject: 4,
    };
    assert_eq!(
        a.region(),
        b.region(),
        "subject-predicate keys share their graph's region"
    );
    assert_eq!(a.region().keyspace(), Keyspace::SubjectPredicates);
    assert!(Region::of(&[0x99]).is_err());
}
