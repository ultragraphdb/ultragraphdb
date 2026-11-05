/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A record's section 4 replayed: each registration the transaction relied on written as its mark at the record's
//! commit version, beside the record's other writes.

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, Sequence, SystemId, TypePrefix};
use ultragraphdb_store_apply::{Applied, MarkedReference, RecordWrites};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::changesets::{TENANT, changesets};
use crate::utils::partitions::{bucket_of, mark_key};

fn reference(group: u32, generation: u64) -> TestResult<MarkedReference> {
    let entity = SystemId::new(TypePrefix::RESOURCE, bucket_of("o")?, Sequence::new(5)?);
    Ok(MarkedReference::new(
        entity,
        BucketGroup::new(group)?,
        NonZeroU64::new(generation).context("a generation")?,
    ))
}

#[test]
fn test_a_record_without_section_four_writes_no_mark() -> TestResult {
    let a = changesets()?;
    let replayed = a.prepare_replay(TENANT, RecordWrites::default())?;
    assert!(replayed.marks().is_empty());
    assert!(replayed.writes().entries().is_empty());
    Ok(())
}

#[test]
fn test_replay_writes_each_mark_of_section_four_at_the_record_s_version() -> TestResult {
    let a = changesets()?;
    let marks = vec![reference(7, 4)?, reference(9, 1)?];
    let record =
        RecordWrites::new(Vec::new(), Vec::new(), Vec::new(), Vec::new()).with_marks(marks.clone());
    let replayed = a.prepare_replay(TENANT, record)?;
    assert_eq!(replayed.marks(), marks.as_slice());
    let written: Vec<&[u8]> = replayed.writes().entry_keys().collect();
    for m in &marks {
        assert!(written.contains(&mark_key(m.entity(), m.group()).as_slice()));
    }
    let at = a.quads().lists().next_version();
    let _applied: Vec<Applied> = a.apply_round(vec![(at, replayed)])?;
    for m in &marks {
        assert_eq!(
            a.quads().mark(TENANT, m.entity(), m.group(), at)?,
            Some(m.generation())
        );
        assert_eq!(a.quads().mark(TENANT, m.entity(), m.group(), at - 1)?, None);
    }
    Ok(())
}

#[test]
fn test_a_logical_replay_writes_each_mark_of_section_four() -> TestResult {
    let a = changesets()?;
    let marks = vec![reference(7, 4)?];
    let writes = ultragraphdb_store_apply::LogicalWrites::new(Vec::new(), Vec::new(), Vec::new())
        .with_marks(marks.clone());
    assert_eq!(writes.marks(), marks.as_slice());
    let replayed = a.prepare_logical(TENANT, &writes, &[])?;
    assert_eq!(replayed.marks(), marks.as_slice());
    let at = a.quads().lists().next_version();
    let _applied: Vec<Applied> = a.apply_round(vec![(at, replayed)])?;
    let m = marks.first().context("one mark")?;
    assert_eq!(
        a.quads().mark(TENANT, m.entity(), m.group(), at)?,
        Some(m.generation())
    );
    Ok(())
}
