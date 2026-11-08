/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What compute takes from a ComputeRead (`PROTOCOL.md` §13.12): its bound slots, the columns its unbound slots make, a read-write
//! transaction's terms and scope, and the deadline its Scans wait by.

use std::num::NonZeroU64;

use ultragraphdb_common_compute::read_times_of;
use ultragraphdb_core_ids::{Bucket, BucketGroup, Epoch, Version};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{ReadLevel, ResumeKeyView};
use ultragraphdb_store_quads::{ReferrerSet, term_key};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::played_reads::played_cluster::{PlayedCluster, run};
use crate::played_reads::played_data::{
    Placed, first_bucket, ids_list, place, reverse_key, tenant_over,
};
use crate::played_reads::played_partition::PlayedPartition;
use crate::played_reads::read_answer::{DEADLINE_MS, Pattern, ReadAnswer, answer_on, iri};

const SUBJECT: &str = "http://ex.org/s";
const PREDICATE: &str = "http://ex.org/p";
const OBJECTS: [&str; 2] = ["http://ex.org/o1", "http://ex.org/o2"];

/// The transaction a read-write read names.
const TRANSACTION: [u8; 16] = [7; 16];

/// A cluster of one partition holding `<s> <p>` listing two objects, of a tenant of its own.
async fn one_list() -> TestResult<(PlayedCluster, u64, Placed)> {
    let cluster = PlayedCluster::start(1, read_times_of(None, None)).await?;
    let tenant = tenant_over(1, 1)?;
    let placed = place(
        cluster.partition(1)?,
        tenant,
        (first_bucket(tenant), 1),
        (SUBJECT, PREDICATE),
        &OBJECTS,
    )?;
    Ok((cluster, tenant, placed))
}

/// `pattern`'s answer read-only at latest level.
async fn latest(cluster: &PlayedCluster, tenant: u64, pattern: &Pattern) -> TestResult<ReadAnswer> {
    let read = pattern.read(tenant, true, ReadLevel::Latest)?;
    answer_on(&mut cluster.client().await?, &read).await
}

/// A read's columns are its unbound slots and the reifier's (`PROTOCOL.md` §13.12, Result `columns`), whichever slots are bound.
#[test]
fn test_a_reads_columns_are_its_unbound_slots_and_the_reifier() -> TestResult {
    run(async {
        let (cluster, tenant, _placed) = one_list().await?;
        let bound = latest(
            &cluster,
            tenant,
            &Pattern::of(Some(SUBJECT), Some(PREDICATE), None),
        )
        .await?;
        assert_eq!(bound.whole()?.columns, 2);
        assert_eq!(bound.column(0), OBJECTS);
        let subjects = latest(&cluster, tenant, &Pattern::of(None, Some(PREDICATE), None)).await?;
        assert_eq!(subjects.whole()?.columns, 3);
        assert_eq!(subjects.column(0), [SUBJECT, SUBJECT]);
        assert_eq!(subjects.column(1), OBJECTS);
        let mut any_graph = Pattern::of(Some(SUBJECT), Some(PREDICATE), None);
        any_graph.graph = None;
        let graphs = latest(&cluster, tenant, &any_graph).await?;
        assert_eq!(
            graphs.whole()?.columns,
            3,
            "the graph, the object and the reifier"
        );
        assert_eq!(graphs.column(1), OBJECTS);
        Ok(())
    })
}

/// A bound IRI object is read from its referrers' reverse lists, the subject its one unbound slot.
#[test]
fn test_a_bound_object_is_read_by_its_referrers() -> TestResult {
    run(async {
        let (cluster, tenant, placed) = one_list().await?;
        let part = cluster.partition(1)?;
        let object = placed.objects.first().copied().context("an object")?;
        refer(part, tenant, &placed, object)?;
        let object_iri = OBJECTS.first().copied().context("an object's IRI")?;
        let answer = latest(
            &cluster,
            tenant,
            &Pattern::of(None, Some(PREDICATE), Some(iri(object_iri))),
        )
        .await?;
        assert_eq!(answer.whole()?.columns, 2, "the subject and the reifier");
        assert_eq!(answer.column(0), [SUBJECT]);
        Ok(())
    })
}

/// Has `part` hold `<s> <p> <object>` by its reverse list and the object's referrer set, its one member the subject's group.
fn refer(
    part: &PlayedPartition,
    tenant: u64,
    placed: &Placed,
    object: ultragraphdb_core_ids::SystemId,
) -> TestResult {
    let encoded = EncodedTerm::id(object);
    part.put(
        reverse_key(tenant, placed.predicate, &encoded),
        ids_list(&[placed.subject]),
    );
    let mut set = ReferrerSet::new();
    let group = BucketGroup::of(Bucket::new(first_bucket(tenant))?);
    set.set(group, NonZeroU64::MIN, Epoch::new(1)?);
    let mut value = Vec::new();
    set.encode(&mut value);
    let key = QuadKey::Referrers {
        tenant,
        entity: object,
    }
    .to_bytes();
    part.put(key, value);
    Ok(())
}

/// A read-only read with no term to resolve still opens with a dictionary Scan, one range of the tenant's empty term's key, which
/// shows the partition has closed the read's epoch; a read-write transaction's, whose Bind did, asks none.
#[test]
fn test_only_a_read_only_read_with_no_term_waits_on_a_dictionary_scan() -> TestResult {
    run(async {
        let (cluster, tenant, _placed) = one_list().await?;
        let part = cluster.partition(1)?;
        let any = term_key(tenant, "");
        let asked_any = |played: &PlayedPartition| {
            played
                .scans()
                .iter()
                .any(|scan| scan.ranges.iter().any(|range| range.first == any))
        };
        let pattern = Pattern::of(None, None, None);
        let written = pattern
            .read(tenant, false, ReadLevel::Snapshot)?
            .with_partition(1)
            .with_transaction(TRANSACTION)
            .with_read_version(Version::bound_of(Epoch::new(5)?));
        let answer = answer_on(&mut cluster.client().await?, &written).await?;
        assert_eq!(answer.whole()?.columns, 4);
        assert_eq!(answer.column(2), OBJECTS);
        assert!(
            !asked_any(part),
            "a read-write read asks no dictionary Scan without a term"
        );
        let read = latest(&cluster, tenant, &pattern).await?;
        assert_eq!(read.column(2), OBJECTS);
        assert!(
            asked_any(part),
            "a read-only read waits on a dictionary Scan"
        );
        Ok(())
    })
}

/// A read-write transaction's dictionary Scan names its IRIs in its term table, so its partition resolves them by their text, and
/// holds no version of its own, which the Bind holds (`PROTOCOL.md` §13.13).
#[test]
fn test_a_read_write_read_names_its_iris_and_holds_no_version() -> TestResult {
    run(async {
        let (cluster, tenant, _placed) = one_list().await?;
        let part = cluster.partition(1)?;
        let written = Pattern::of(Some(SUBJECT), Some(PREDICATE), None)
            .read(tenant, false, ReadLevel::Snapshot)?
            .with_partition(1)
            .with_transaction(TRANSACTION)
            .with_read_version(Version::bound_of(Epoch::new(5)?));
        let answer = answer_on(&mut cluster.client().await?, &written).await?;
        assert_eq!(answer.column(0), OBJECTS);
        let scans = part.scans();
        let gate = scans.first().context("the dictionary Scan")?;
        assert_eq!(gate.terms, 2, "the subject and the predicate");
        assert!(scans.iter().all(|scan| !scan.holds_a_version()));
        Ok(())
    })
}

/// A resume key is bound to its transaction (`PROTOCOL.md` §11.8, `scope`): a read-only read's by its read version, a read-write
/// transaction's by XXH3-64 of its identity.
#[test]
fn test_a_resume_key_is_bound_to_its_transaction() -> TestResult {
    run(async {
        let (cluster, tenant, _placed) = one_list().await?;
        let pattern = Pattern::of(Some(SUBJECT), Some(PREDICATE), None);
        let version = Version::bound_of(Epoch::new(5)?);
        let snapshot = pattern
            .read(tenant, true, ReadLevel::Snapshot)?
            .with_read_version(version)
            .with_epoch(5)
            .with_limit(1);
        let first = answer_on(&mut cluster.client().await?, &snapshot).await?;
        let key = first.resume();
        assert_eq!(ResumeKeyView::read(&key)?.scope(), version);
        let written = pattern
            .read(tenant, false, ReadLevel::Snapshot)?
            .with_partition(1)
            .with_transaction(TRANSACTION)
            .with_read_version(version)
            .with_limit(1);
        let second = answer_on(&mut cluster.client().await?, &written).await?;
        let written_key = second.resume();
        assert_eq!(
            ResumeKeyView::read(&written_key)?.scope(),
            xxhash_rust::xxh3::xxh3_64(&TRANSACTION)
        );
        Ok(())
    })
}

/// A Scan waiting for a snapshot's epoch waits at most the time left until the transaction's deadline (`PROTOCOL.md` §13.13,
/// `deadline_ms`), and one that waits for no epoch names none.
#[test]
fn test_a_scan_waits_for_its_epoch_within_the_reads_deadline() -> TestResult {
    run(async {
        let (cluster, tenant, _placed) = one_list().await?;
        let read = Pattern::of(Some(SUBJECT), Some(PREDICATE), None)
            .read(tenant, true, ReadLevel::Snapshot)?
            .with_read_version(Version::bound_of(Epoch::new(5)?))
            .with_epoch(5);
        let answer = answer_on(&mut cluster.client().await?, &read).await?;
        assert_eq!(answer.column(0), OBJECTS);
        let scans = cluster.partition(1)?.read_scans();
        let gate = scans.first().context("the dictionary Scan")?;
        assert_eq!(gate.epoch, 5);
        assert!(
            gate.deadline_ms <= DEADLINE_MS && gate.deadline_ms > DEADLINE_MS - 10_000,
            "{} within the read's {DEADLINE_MS}",
            gate.deadline_ms
        );
        for scan in scans.iter().filter(|scan| scan.epoch == 0) {
            assert_eq!(scan.deadline_ms, 0);
        }
        Ok(())
    })
}
