/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{Bucket, BucketGroup, Sequence, SystemId, TypePrefix};
use ultragraphdb_store_apply::{EntityRef, MarkedReference};
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_a_marked_reference_holds_its_entity_group_and_generation() -> TestResult {
    let entity = SystemId::new(TypePrefix::RESOURCE, Bucket::new(64)?, Sequence::new(3)?);
    let group = BucketGroup::new(1)?;
    let generation = NonZeroU64::new(4).context("4 is no zero")?;
    let marked = MarkedReference::new(entity, group, generation);
    assert_eq!(marked.entity(), entity);
    assert_eq!(marked.group(), group);
    assert_eq!(marked.generation(), generation);
    Ok(())
}

#[test]
fn test_an_entity_is_named_by_its_iri_or_its_id() -> TestResult {
    let entity = SystemId::new(TypePrefix::RESOURCE, Bucket::new(64)?, Sequence::new(3)?);
    assert_ne!(EntityRef::Iri("http://ex.org/o"), EntityRef::Id(entity));
    assert_eq!(EntityRef::Id(entity), EntityRef::Id(entity));
    Ok(())
}
