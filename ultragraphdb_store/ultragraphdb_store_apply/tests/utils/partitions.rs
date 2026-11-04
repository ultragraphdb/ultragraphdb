/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the partition tests share: an applier owning all buckets but one, commit versions of a chosen epoch, IRIs'
//! buckets and groups as core_ids derives them, and the keys of marks and referrer sets.

use ultragraphdb_core_ids::{
    Bucket, BucketGroup, CanonicalKey, Epoch, Namespace, SystemId, TypePrefix, Version,
};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_posting::{Lists, ListsConfig};
use ultragraphdb_store_quads::{OwnedBuckets, QuadStore};
use ultragraphdb_store_sim::{Seed, SimListCache, SteppedExecutor, Stream};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::changesets::{NS, TestChangesets, blocks, ex, sim_store};

/// The bucket of `<name>` in the tests' namespace, as core_ids derives it.
pub fn bucket_of(name: &str) -> TestResult<Bucket> {
    let iri = ex(name);
    Ok(Bucket::derive(
        Namespace::new(NS),
        TypePrefix::RESOURCE,
        &CanonicalKey::iri(iri.as_str())?,
    ))
}

/// The bucket group of `<name>`'s bucket.
pub fn group_of(name: &str) -> TestResult<BucketGroup> {
    Ok(BucketGroup::of(bucket_of(name)?))
}

/// An applier over a fresh simulated store owning every bucket but `<name>`'s, as one partition of several does.
pub fn owning_all_but(name: &str) -> TestResult<TestChangesets> {
    let b = bucket_of(name)?.get();
    let mut ranges = Vec::new();
    if b > 0 {
        ranges.push((Bucket::new(0)?, Bucket::new(b - 1)?));
    }
    if b < Bucket::MAX.get() {
        ranges.push((Bucket::new(b + 1)?, Bucket::MAX));
    }
    let owned = OwnedBuckets::new(&ranges).context("disjoint ranges")?;
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let quads = QuadStore::open(
        Lists::new(sim_store(), cache, ListsConfig::default()),
        blocks(),
        RandomSource::new(1),
    )?
    .owning(owned);
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    Ok(ChangesetApplier::over(quads, executor, 1)?)
}

/// The commit version of sequence `sequence` in epoch `epoch`.
pub fn version(epoch: u64, sequence: u32) -> TestResult<u64> {
    Ok(Version::commit(Epoch::new(epoch)?, sequence)?.to_u64())
}

/// The epoch `epoch`.
pub fn epoch(epoch: u64) -> TestResult<Epoch> {
    Ok(Epoch::new(epoch)?)
}

/// The key of the mark of `(entity, group)`.
pub fn mark_key(entity: SystemId, group: BucketGroup) -> Vec<u8> {
    QuadKey::Mark {
        namespace: NS,
        entity,
        group: group.get(),
    }
    .to_bytes()
}

/// The key of `entity`'s referrer set.
pub fn referrers_key(entity: SystemId) -> Vec<u8> {
    QuadKey::Referrers {
        namespace: NS,
        entity,
    }
    .to_bytes()
}
