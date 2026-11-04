/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::Reference;
use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_store_apply::EntityRef;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_reference_names_its_entity_by_id_when_held_and_by_iri_otherwise() -> TestResult {
    let home = PartitionId::new(2)?;
    let group = BucketGroup::new(7)?;
    let iri = "http://ex.org/b".to_string();
    let unknown = Reference::new(home, 3, iri.clone(), None, Some(group), 0);
    assert_eq!(
        (
            unknown.home(),
            unknown.namespace(),
            unknown.iri(),
            unknown.entity()
        ),
        (home, 3, iri.as_str(), None)
    );
    assert_eq!((unknown.group(), unknown.named()), (Some(group), 0));
    assert_eq!(unknown.entity_ref(), EntityRef::Iri(&iri));
    let id = SystemId::from_u128((1 << 112) | (9 << 48) | (5 << 24))?;
    let known = Reference::new(home, 3, iri.clone(), Some(id), None, 4);
    assert_eq!(known.entity_ref(), EntityRef::Id(id));
    assert_eq!((known.group(), known.named()), (None, 4));
    Ok(())
}
