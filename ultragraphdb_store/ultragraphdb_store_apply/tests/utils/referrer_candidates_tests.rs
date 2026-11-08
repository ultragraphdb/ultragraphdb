/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The referrer candidates of a replayed record: an edge's object joins its referrer set as the subject's group, and a
//! list under a key that is no forward list's, or a reference posting naming no system ID, which no write logs, names
//! no candidate.

use ultragraphdb_core_codec::PostingList;
use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_core_keys::{Keyspace, QuadKey};
use ultragraphdb_store_apply::{Applied, RecordWrites};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::changesets::{TENANT, changeset, changesets, insert};

/// The forward list `<a> <knows> <b>` writes, its key and encoded delta, with the IDs of `<a>` and `<b>`.
fn knows() -> TestResult<(Vec<u8>, PostingList, SystemId, SystemId)> {
    let first = changesets()?;
    let executed = first.execute(&changeset(vec![insert("<a> <knows> <b>")?]))?;
    let (key, delta) = executed
        .writes()
        .deltas()
        .iter()
        .find(|(k, _)| k.first() == Some(&Keyspace::Forward.byte()))
        .cloned()
        .context("the forward list")?;
    let Ok((QuadKey::Forward { subject, .. }, None)) = QuadKey::parse(&key) else {
        return Err(TestError::new("no forward key".to_string()));
    };
    let object = delta
        .postings()
        .first()
        .map(|p| SystemId::from_u128(p.uid()))
        .context("a posting")??;
    first.release(&executed);
    Ok((key, delta, subject, object))
}

/// The groups of `object`'s referrer set in a fresh store after it replays the one list `key` holding `delta`.
fn groups_after_replay(
    key: &[u8],
    delta: &PostingList,
    object: SystemId,
) -> TestResult<Vec<BucketGroup>> {
    let replica = changesets()?;
    let value = delta.encode();
    let record = RecordWrites::new(
        vec![(key, value.as_slice())],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let replayed = replica.prepare_replay(TENANT, record)?;
    let version = replica.quads().lists().next_version();
    let _applied: Vec<Applied> = replica.apply_round(vec![(version, replayed)])?;
    Ok(replica
        .quads()
        .referrers(TENANT, object, replica.quads().lists().applied_ts())?
        .members()
        .iter()
        .map(ultragraphdb_store_quads::ReferrerMember::group)
        .collect())
}

#[test]
fn test_a_replayed_edge_joins_its_objects_referrer_set() -> TestResult {
    let (key, delta, subject, object) = knows()?;
    assert_eq!(
        groups_after_replay(&key, &delta, object)?,
        vec![BucketGroup::of(subject.bucket())]
    );
    Ok(())
}

#[test]
fn test_a_list_under_a_key_that_is_no_forward_lists_names_no_candidate() -> TestResult {
    let (key, delta, _, object) = knows()?;
    let short = key.get(..key.len() - 1).context("a shorter key")?;
    assert!(QuadKey::parse(short).is_err(), "no key parses");
    assert!(groups_after_replay(short, &delta, object)?.is_empty());
    Ok(())
}

#[test]
fn test_a_reference_posting_naming_no_system_id_names_no_candidate() -> TestResult {
    let (key, mut delta, _, object) = knows()?;
    let posting = delta.postings_mut().first_mut().context("a posting")?;
    // Type prefix 0: no system ID.
    *posting = posting.clone().with_uid(1);
    assert!(SystemId::from_u128(1).is_err(), "1 is no system ID");
    assert!(groups_after_replay(&key, &delta, object)?.is_empty());
    Ok(())
}
