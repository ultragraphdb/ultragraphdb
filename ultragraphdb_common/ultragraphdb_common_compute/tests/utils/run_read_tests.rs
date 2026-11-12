/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read run to its end (`PROTOCOL.md` §13.12, §11.8, §11.9, §11.12): the version each partition reads, the epoch and partition
//! its batches name, what holds its versions, the merge of its partitions' rows in key order, its limit and its resume key.

use std::num::NonZeroU64;

use ultragraphdb_common_compute::read_times_of;
use ultragraphdb_core_ids::{Bucket, BucketGroup, Epoch, SystemId, Version};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{
    BodyView, ComputeRead, ErrorCode, ErrorReplyView, FLAG_LAST, MAX_FRAME_FLOOR,
    MAX_FRAME_FLOOR_U32, ReadLevel, ResumeKey, ResumeKeyView, ResumeRow, ScanAt, TermTable,
};
use ultragraphdb_store_quads::{ReferrerSet, term_key};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::apart::apart_cluster::links_holding_streams;
use crate::played_reads::played_cluster::{PlayedCluster, run};
use crate::played_reads::played_data::{
    bucket_at, first_bucket, forward_key, home_of, id_in, ids_list, iri_at, name, name_key, place,
    reverse_key, subject_predicates_key, tenant_over,
};
use crate::played_reads::played_partition::PLAYED_EPOCH;
use crate::played_reads::read_answer::{
    DEADLINE_MS, Pattern, ReadAnswer, answer_on, iri, next_frame,
};

const SUBJECT: &str = "http://ex.org/s";
const PREDICATE: &str = "http://ex.org/p";
const OBJECTS: [&str; 3] = ["http://ex.org/o1", "http://ex.org/o2", "http://ex.org/o3"];

/// The epoch a snapshot read reads at, below the played partitions' own.
const SNAPSHOT: u64 = 5;

/// `pattern` read-only at snapshot level at [`SNAPSHOT`].
fn snapshot(pattern: &Pattern, tenant: u64) -> TestResult<ComputeRead> {
    Ok(pattern
        .read(tenant, true, ReadLevel::Snapshot)?
        .with_read_version(Version::bound_of(Epoch::new(SNAPSHOT)?))
        .with_epoch(SNAPSHOT))
}

/// A cluster of one partition holding `<s> <p>` listing three objects, and its tenant.
async fn one_list() -> TestResult<(PlayedCluster, u64)> {
    let cluster = PlayedCluster::start(1, read_times_of(None, None)).await?;
    let tenant = tenant_over(1, 1)?;
    let _placed = place(
        cluster.partition(1)?,
        tenant,
        (first_bucket(tenant), 1),
        (SUBJECT, PREDICATE),
        &OBJECTS,
    )?;
    Ok((cluster, tenant))
}

/// `read`'s answer on a connection of its own.
async fn answer(cluster: &PlayedCluster, read: &ComputeRead) -> TestResult<ReadAnswer> {
    answer_on(&mut cluster.client().await?, read).await
}

/// A read at a snapshot names its epoch and its one partition in its batches, and holds its version from its first frame on by
/// a Scan of its own (quad-reads, "A partition retains what a read may still read"); a read at latest level, which holds a
/// version only once it outlasts a short read, names neither and holds none.
#[test]
fn test_a_snapshot_read_names_its_epoch_and_holds_its_version() -> TestResult {
    run(async {
        let (cluster, tenant) = one_list().await?;
        let part = cluster.partition(1)?;
        let pattern = Pattern::of(Some(SUBJECT), Some(PREDICATE), None);
        let read = answer(&cluster, &snapshot(&pattern, tenant)?).await?;
        assert_eq!(read.column(0), OBJECTS);
        let last = read.whole()?;
        assert_eq!((last.epoch, last.partition), (SNAPSHOT, 1));
        let held = || {
            part.scans()
                .iter()
                .filter(|scan| scan.holds_a_version())
                .count()
        };
        assert_eq!(held(), 1, "the snapshot's version held at once");
        let latest = answer(&cluster, &pattern.read(tenant, true, ReadLevel::Latest)?).await?;
        assert_eq!(latest.whole()?.epoch, 0);
        assert_eq!(held(), 1, "a read at latest level holds none at once");
        Ok(())
    })
}

/// A read-write transaction's batches and a named version's name epoch 0 (`PROTOCOL.md` §13.12, Result `epoch`), whatever epoch
/// their ComputeRead carries; a named version is held at once.
#[test]
fn test_a_read_write_or_named_read_names_epoch_zero() -> TestResult {
    run(async {
        let (cluster, tenant) = one_list().await?;
        let part = cluster.partition(1)?;
        let pattern = Pattern::of(Some(SUBJECT), Some(PREDICATE), None);
        let version = Version::bound_of(Epoch::new(SNAPSHOT)?);
        let written = pattern
            .read(tenant, false, ReadLevel::Snapshot)?
            .with_partition(1)
            .with_transaction([3; 16])
            .with_read_version(version)
            .with_epoch(SNAPSHOT);
        let read = answer(&cluster, &written).await?;
        assert_eq!(read.column(0), OBJECTS);
        assert_eq!(read.whole()?.epoch, 0);
        assert!(part.scans().iter().all(|scan| !scan.holds_a_version()));
        let named = pattern
            .read(tenant, true, ReadLevel::Snapshot)?
            .with_named_version(version)
            .with_epoch(SNAPSHOT);
        let before = part.read_scans().len();
        let named_read = answer(&cluster, &named).await?;
        assert_eq!(named_read.whole()?.epoch, 0);
        let scans = part.read_scans();
        assert_eq!(scans.get(before).map(|scan| scan.at), Some(ScanAt::Named));
        assert!(part.scans().iter().any(|scan| scan.holds_a_version()));
        Ok(())
    })
}

/// A read stops at its limit, with a resume key while a row may be left and with none once its partition read to its end; the
/// key continues the read after its last row, neither repeating nor skipping one (`PROTOCOL.md` §11.8).
#[test]
fn test_a_limited_read_resumes_after_its_last_row() -> TestResult {
    run(async {
        let (cluster, tenant) = one_list().await?;
        let pattern = Pattern::of(Some(SUBJECT), Some(PREDICATE), None);
        let first = answer(&cluster, &snapshot(&pattern, tenant)?.with_limit(2)).await?;
        assert_eq!(first.column(0), OBJECTS[..2]);
        let key = first.resume();
        assert!(!key.is_empty(), "a row is left");
        let (terms, slots) = pattern.table()?;
        let resumed = ComputeRead::new(tenant, true, ReadLevel::Snapshot, terms, slots, key)
            .with_principal(1)
            .with_deadline_ms(crate::played_reads::read_answer::DEADLINE_MS)
            .with_client_limits(ultragraphdb_protocol_wire::MAX_FRAME_SIZE_U32, 4)
            .with_read_version(Version::bound_of(Epoch::new(SNAPSHOT)?))
            .with_epoch(SNAPSHOT);
        let rest = answer(&cluster, &resumed).await?;
        assert_eq!(rest.column(0), OBJECTS[2..]);
        let whole = answer(&cluster, &snapshot(&pattern, tenant)?.with_limit(3)).await?;
        assert_eq!(whole.column(0), OBJECTS);
        assert!(
            whole.resume().is_empty(),
            "the partition read its list to its end"
        );
        Ok(())
    })
}

/// A read holds the names of the rows it is about to return, as far as its limit reaches, and reads none past it: of `<s>`'s
/// second predicate's three objects, a read limited to four rows names two.
#[test]
fn test_a_limited_read_names_no_row_past_its_limit() -> TestResult {
    run(async {
        let cluster = PlayedCluster::start(1, read_times_of(None, None)).await?;
        let tenant = tenant_over(1, 1)?;
        let part = cluster.partition(1)?;
        let bucket = first_bucket(tenant);
        let first = place(
            part,
            tenant,
            (bucket, 1),
            (SUBJECT, PREDICATE),
            &OBJECTS[..2],
        )?;
        let second = id_in(bucket, 10)?;
        name(part, tenant, "http://ex.org/q", second);
        let mut objects = Vec::new();
        for (at, object) in (11..).zip(["http://ex.org/o4", "http://ex.org/o5", "http://ex.org/o6"])
        {
            let id = id_in(bucket, at)?;
            name(part, tenant, object, id);
            objects.push(id);
        }
        part.put(
            forward_key(tenant, second, first.subject),
            ids_list(&objects),
        );
        part.put(
            subject_predicates_key(tenant, first.subject),
            ids_list(&[first.predicate, second]),
        );
        let pattern = Pattern::of(Some(SUBJECT), None, None);
        let read = answer(
            &cluster,
            &pattern.read(tenant, true, ReadLevel::Latest)?.with_limit(4),
        )
        .await?;
        assert_eq!(
            read.column(1),
            [
                "http://ex.org/o1",
                "http://ex.org/o2",
                "http://ex.org/o4",
                "http://ex.org/o5"
            ]
        );
        let last = objects.last().copied().context("the last object")?;
        let key = name_key(tenant, last);
        let asked = part
            .scans()
            .iter()
            .flat_map(|scan| scan.ranges.iter())
            .any(|range| range.first == key);
        assert!(!asked, "the sixth object's name lies past the limit");
        Ok(())
    })
}

/// The partitions of a cluster reads reach two of.
const SPREAD: u32 = 2;

/// A cluster of two partitions, a tenant whose run both hold, and the two partitions.
async fn spread() -> TestResult<(PlayedCluster, u64, [u32; 2])> {
    spread_by(read_times_of(None, None)).await
}

/// [`spread`], compute reading by `times`.
async fn spread_by(
    times: ultragraphdb_common_compute::ReadTimes,
) -> TestResult<(PlayedCluster, u64, [u32; 2])> {
    let cluster = PlayedCluster::start(SPREAD, times).await?;
    let tenant = tenant_over(SPREAD, 2)?;
    let map = crate::played_reads::played_data::placement(SPREAD)?;
    let holders: Vec<u32> = map
        .holders_of_run(ultragraphdb_core_ids::Tenant::new(tenant), Epoch::ZERO)
        .into_iter()
        .map(|(holder, _)| holder.get())
        .collect();
    let pair = [
        holders.first().copied().context("a first holder")?,
        holders.get(1).copied().context("a second holder")?,
    ];
    Ok((cluster, tenant, pair))
}

/// Has the partition of each subject hold `<subject> <p>` listing its objects, every IRI's text key at its home as well, the
/// predicate's in both partitions; the subjects' IRIs.
fn spread_lists(
    cluster: &PlayedCluster,
    tenant: u64,
    pair: [u32; 2],
    lists: &[(u32, u64, &[&str])],
) -> TestResult<Vec<String>> {
    let predicate = id_in(bucket_at(SPREAD, tenant, pair[0])?, 1)?;
    for partition in pair {
        name(cluster.partition(partition)?, tenant, PREDICATE, predicate);
    }
    let mut subjects = Vec::new();
    for (partition, sequence, objects) in lists {
        let part = cluster.partition(*partition)?;
        let bucket = bucket_at(SPREAD, tenant, *partition)?;
        let subject = iri_at(SPREAD, tenant, *partition, &format!("s{sequence}-"))?;
        let id = id_in(bucket, *sequence)?;
        name(part, tenant, &subject, id);
        let mut ids = Vec::new();
        for (at, object) in (1..).zip(objects.iter()) {
            let object_id = id_in(bucket, sequence * 100 + at)?;
            name(part, tenant, object, object_id);
            let home = cluster.partition(home_of(SPREAD, tenant, object)?)?;
            home.put(term_key(tenant, object), object_id.to_be_bytes().to_vec());
            ids.push(object_id);
        }
        part.put(forward_key(tenant, predicate, id), ids_list(&ids));
        subjects.push(subject);
    }
    Ok(subjects)
}

/// A read reaching two partitions merges their rows in key order, names partition 0 in its batches, and resumes from its key
/// in each, the key's terms read at their homes (`PROTOCOL.md` §11.8, §13.12).
#[test]
fn test_a_read_over_two_partitions_merges_in_key_order_and_resumes() -> TestResult {
    run(async {
        let (cluster, tenant, pair) = spread().await?;
        let lists: [(u32, u64, &[&str]); 3] = [
            (pair[0], 10, &OBJECTS[..1]),
            (pair[0], 11, &OBJECTS[1..2]),
            (pair[1], 12, &OBJECTS[2..]),
        ];
        let subjects = spread_lists(&cluster, tenant, pair, &lists)?;
        let pattern = Pattern::of(None, Some(PREDICATE), None);
        let read = answer(&cluster, &snapshot(&pattern, tenant)?).await?;
        assert_eq!(read.column(0), subjects);
        assert_eq!(read.column(1), OBJECTS);
        assert_eq!(read.whole()?.partition, 0, "two partitions reached");
        let first = answer(&cluster, &snapshot(&pattern, tenant)?.with_limit(1)).await?;
        let (terms, slots) = pattern.table()?;
        let resumed = ComputeRead::new(
            tenant,
            true,
            ReadLevel::Snapshot,
            terms,
            slots,
            first.resume(),
        )
        .with_principal(1)
        .with_deadline_ms(crate::played_reads::read_answer::DEADLINE_MS)
        .with_client_limits(ultragraphdb_protocol_wire::MAX_FRAME_SIZE_U32, 4)
        .with_read_version(Version::bound_of(Epoch::new(SNAPSHOT)?))
        .with_epoch(SNAPSHOT);
        let rest = answer(&cluster, &resumed).await?;
        assert_eq!(rest.column(1), OBJECTS[1..]);
        Ok(())
    })
}

/// A monotonic-fast read reaching several partitions first asks each the epoch it has closed, every Closed sent before any answer
/// is awaited, and reads each at the lowest one's version bound; a snapshot read, or one reaching a single partition, asks none.
#[test]
fn test_a_monotonic_fast_read_over_partitions_reads_at_their_lowest_close() -> TestResult {
    run(async {
        let (cluster, tenant, pair) = spread().await?;
        let lists: [(u32, u64, &[&str]); 2] =
            [(pair[0], 10, &OBJECTS[..1]), (pair[1], 12, &OBJECTS[1..2])];
        let subjects = spread_lists(&cluster, tenant, pair, &lists)?;
        let (low, high) = (cluster.partition(pair[0])?, cluster.partition(pair[1])?);
        low.close_through(7);
        high.close_through(8);
        let pattern = Pattern::of(None, Some(PREDICATE), None);
        let read = answer(&cluster, &snapshot(&pattern, tenant)?).await?;
        assert_eq!(read.column(1), OBJECTS[..2]);
        assert_eq!(
            (low.closed_asked(), high.closed_asked()),
            (0, 0),
            "a snapshot asks no Closed"
        );
        let fast = pattern
            .read(tenant, true, ReadLevel::MonotonicFast)?
            .with_epoch(2);
        let fast_read = answer(&cluster, &fast).await?;
        assert_eq!(fast_read.column(1), OBJECTS[..2]);
        assert_eq!(
            fast_read.whole()?.epoch,
            PLAYED_EPOCH,
            "the epoch its partitions chose"
        );
        assert_eq!((low.closed_asked(), high.closed_asked()), (1, 1));
        let bound = Version::bound_of(Epoch::new(7)?);
        for part in [low, high] {
            let first = part
                .read_scans()
                .into_iter()
                .find(|scan| scan.epoch == 7)
                .context("a Scan at the lowest close")?;
            assert_eq!((first.at, first.read_version), (ScanAt::ReadVersion, bound));
        }
        let subject = subjects.first().context("a subject")?;
        let one = Pattern::of(Some(subject), Some(PREDICATE), None);
        let before = low.read_scans().len();
        let alone = answer(
            &cluster,
            &one.read(tenant, true, ReadLevel::MonotonicFast)?
                .with_epoch(2),
        )
        .await?;
        assert_eq!(alone.column(0), OBJECTS[..1]);
        assert_eq!(
            low.closed_asked(),
            1,
            "one partition reached asks no Closed"
        );
        let first = low
            .read_scans()
            .get(before)
            .map(|scan| (scan.at, scan.epoch));
        assert_eq!(first, Some((ScanAt::LatestClosed, 2)));
        Ok(())
    })
}

/// A reverse read asks the object's home for its referrer set, then the partitions holding the set's members, each first read
/// as the read's level says, and names partition 0 for the two partitions it reached (`PROTOCOL.md` §11.12, quad-reads "A reverse
/// read asks the object's referrers").
#[test]
fn test_a_reverse_read_asks_the_partitions_its_referrer_set_names() -> TestResult {
    run(async {
        let (cluster, tenant, pair) = spread().await?;
        let object = iri_at(SPREAD, tenant, pair[0], "object-")?;
        let (home, member) = (cluster.partition(pair[0])?, cluster.partition(pair[1])?);
        let bucket = bucket_at(SPREAD, tenant, pair[1])?;
        let (subject, predicate, object_id) = (
            id_in(bucket, 1)?,
            id_in(bucket, 2)?,
            id_in(bucket_at(SPREAD, tenant, pair[0])?, 3)?,
        );
        name(home, tenant, &object, object_id);
        name(home, tenant, PREDICATE, predicate);
        name(member, tenant, PREDICATE, predicate);
        name(member, tenant, SUBJECT, subject);
        member.put(
            reverse_key(tenant, predicate, &EncodedTerm::id(object_id)),
            ids_list(&[subject]),
        );
        let (key, value) = referrers_of(tenant, object_id, bucket)?;
        home.put(key, value);
        let pattern = Pattern::of(None, Some(PREDICATE), Some(iri(&object)));
        let read = answer(&cluster, &snapshot(&pattern, tenant)?).await?;
        assert_eq!(read.column(0), [SUBJECT]);
        assert_eq!(
            read.whole()?.partition,
            0,
            "the home and the member reached"
        );
        let first = member
            .read_scans()
            .into_iter()
            .next()
            .context("the member's first Scan")?;
        let version = Version::bound_of(Epoch::new(SNAPSHOT)?);
        assert_eq!(
            (first.at, first.read_version, first.epoch),
            (ScanAt::ReadVersion, version, SNAPSHOT)
        );
        Ok(())
    })
}

/// The referrer set's key of `object` and its value, its one member the group of `bucket`.
fn referrers_of(tenant: u64, object: SystemId, bucket: u32) -> TestResult<(Vec<u8>, Vec<u8>)> {
    let mut set = ReferrerSet::new();
    set.set(
        BucketGroup::of(Bucket::new(bucket)?),
        NonZeroU64::MIN,
        Epoch::new(1)?,
    );
    let mut value = Vec::new();
    set.encode(&mut value);
    let key = QuadKey::Referrers {
        tenant,
        entity: object,
    }
    .to_bytes();
    Ok((key, value))
}

/// A monotonic-fast read reaching two partitions asks each the epoch it has closed on a stream of its own, which leaves that
/// partition's link once answered: past the read, the core keeps both links and neither holds a stream (`PROTOCOL.md` §11.3,
/// §11.12).
#[test]
fn test_a_monotonic_fast_read_leaves_no_stream_on_its_links() -> TestResult {
    run(async {
        let (cluster, tenant, pair) = spread().await?;
        let lists: [(u32, u64, &[&str]); 2] =
            [(pair[0], 10, &OBJECTS[..1]), (pair[1], 12, &OBJECTS[1..2])];
        let _subjects = spread_lists(&cluster, tenant, pair, &lists)?;
        let fast = Pattern::of(None, Some(PREDICATE), None)
            .read(tenant, true, ReadLevel::MonotonicFast)?
            .with_epoch(2);
        let read = answer(&cluster, &fast).await?;
        assert_eq!(read.column(1), OBJECTS[..2]);
        let (low, high) = (cluster.partition(pair[0])?, cluster.partition(pair[1])?);
        assert_eq!(
            (low.closed_asked(), high.closed_asked()),
            (1, 1),
            "a Closed of each"
        );
        assert_eq!(
            links_holding_streams(&cluster.core).await,
            (2, 0),
            "two links, holding no stream"
        );
        Ok(())
    })
}

/// A resume key continues the read it came from alone: one carried to another pattern, or to a read of another version, is
/// refused with Error 1 before any partition is asked (`PROTOCOL.md` §11.8).
#[test]
fn test_a_resume_key_of_another_pattern_or_version_is_refused() -> TestResult {
    run(async {
        let (cluster, tenant) = one_list().await?;
        let pattern = Pattern::of(Some(SUBJECT), Some(PREDICATE), None);
        let first = answer(&cluster, &snapshot(&pattern, tenant)?.with_limit(1)).await?;
        let key = first.resume();
        assert!(!key.is_empty(), "a row is left");
        let asked = cluster.partition(1)?.read_scans().len();
        let other = Pattern::of(Some(SUBJECT), None, None);
        let at = |of: &Pattern, epoch: u64| -> TestResult<ComputeRead> {
            let (terms, slots) = of.table()?;
            Ok(
                ComputeRead::new(tenant, true, ReadLevel::Snapshot, terms, slots, key.clone())
                    .with_principal(1)
                    .with_deadline_ms(crate::played_reads::read_answer::DEADLINE_MS)
                    .with_client_limits(ultragraphdb_protocol_wire::MAX_FRAME_SIZE_U32, 4)
                    .with_read_version(Version::bound_of(Epoch::new(epoch)?))
                    .with_epoch(epoch),
            )
        };
        for read in [at(&other, SNAPSHOT)?, at(&pattern, SNAPSHOT - 1)?] {
            let refused = answer(&cluster, &read).await?;
            assert_eq!(
                refused.refusal()?.code,
                ultragraphdb_protocol_wire::ErrorCode::Protocol
            );
        }
        assert_eq!(
            cluster.partition(1)?.read_scans().len(),
            asked,
            "no partition asked"
        );
        Ok(())
    })
}

/// A resume key naming a term no home holds is refused with Error 1 naming the term, within the read's `max_frame` though the
/// term is longer than that frame: the message is cut to fit, so the read ends with its refusal (`PROTOCOL.md` §9.4, §11.8).
#[test]
fn test_a_resume_key_naming_an_unheld_term_past_the_frame_is_refused_within_it() -> TestResult {
    run(async {
        let (cluster, tenant) = one_list().await?;
        let pattern = Pattern::of(Some(SUBJECT), Some(PREDICATE), None);
        let first = answer(&cluster, &snapshot(&pattern, tenant)?.with_limit(1)).await?;
        let held = first.resume();
        let key = ResumeKeyView::read(&held)?;
        let unheld = format!("{SUBJECT}/{}", "u".repeat(MAX_FRAME_FLOOR));
        let mut terms = TermTable::new();
        let row = ResumeRow::new(
            None,
            terms.iri(&unheld),
            terms.iri(PREDICATE),
            terms.iri(OBJECTS[0]),
            None,
        );
        let forged = ResumeKey::new(key.pattern(), key.scope(), terms, row).encode();
        let (table, slots) = pattern.table()?;
        let resumed = ComputeRead::new(tenant, true, ReadLevel::Snapshot, table, slots, forged)
            .with_principal(1)
            .with_deadline_ms(DEADLINE_MS)
            .with_client_limits(MAX_FRAME_FLOOR_U32, 4)
            .with_read_version(Version::bound_of(Epoch::new(SNAPSHOT)?))
            .with_epoch(SNAPSHOT);
        let mut client = cluster.client().await?;
        client.send_only(&resumed).await?;
        let frame = next_frame(&mut client).await?;
        assert!(
            frame.bytes().len() <= MAX_FRAME_FLOOR,
            "the refusal fits the read's max_frame: {} bytes",
            frame.bytes().len()
        );
        assert_ne!(frame.flags() & FLAG_LAST, 0, "the refusal ends the read");
        let view = frame.view()?;
        let refused = ErrorReplyView::from_frame(&view)?.to_body()?;
        assert_eq!(refused.code(), ErrorCode::Protocol);
        let message = refused.message();
        let at = message
            .find(
                unheld
                    .get(..SUBJECT.len() + 2)
                    .context("the term's start")?,
            )
            .context("the term named")?;
        let named = message.get(at..).context("the term named")?;
        assert!(
            unheld.starts_with(named),
            "the message ends in the term's start"
        );
        assert!(named.len() < unheld.len(), "the term is cut");
        Ok(())
    })
}

/// A monotonic-fast read over partitions, its partitions dialled by a read before it, is refused when one fails the Closed it is
/// asked: one silent past the answer bound with code 17, its connection closed; one refusing with its own refusal; one answering
/// with a frame that is no Closed, or whose body does not read, with code 17, its connection closed.
#[test]
fn test_a_closed_failed_refuses_a_monotonic_fast_read() -> TestResult {
    use ultragraphdb_protocol_wire::ErrorCode;

    use crate::played_reads::played_partition::ClosedAnswer;

    run(async {
        let refused = ClosedAnswer::Refuse(ErrorCode::TooOld, 7);
        let answers = [
            (ClosedAnswer::Silent, ErrorCode::Unavailable),
            (refused, ErrorCode::TooOld),
            (ClosedAnswer::NotClosed, ErrorCode::Unavailable),
            (ClosedAnswer::Corrupt, ErrorCode::Unavailable),
        ];
        for (closed, code) in answers {
            // A partition silent past 1 s is hung: well past a dial on a loaded host, which a first read makes before the Closed.
            let (cluster, tenant, pair) = spread_by(read_times_of(Some(500), None)).await?;
            let pattern = Pattern::of(None, Some(PREDICATE), None);
            answer(&cluster, &snapshot(&pattern, tenant)?)
                .await?
                .ended()?;
            let failing = cluster.partition(pair[0])?;
            failing.answer_closed_with(closed);
            let read = pattern.read(tenant, true, ReadLevel::MonotonicFast)?;
            let answered = answer(&cluster, &read).await?;
            let refusal = answered.refusal()?;
            assert_eq!(refusal.code, code, "{closed:?}: {refusal:?}");
            if closed == refused {
                assert_eq!(refusal.detail, 7, "the partition's own detail");
            }
            assert!(failing.closed_asked() >= 1);
        }
        Ok(())
    })
}

/// A reverse read at a named version reads the home's referrer set at the latest version, one at monotonic-fast level reads each
/// member at the epoch the home answered, and both answer the member's row; an object its home does not name answers no row,
/// asking no member (`PROTOCOL.md` §11.12).
#[test]
fn test_a_reverse_read_named_or_monotonic_fast_reads_its_members() -> TestResult {
    run(async {
        let (cluster, tenant, pair) = spread().await?;
        let object = iri_at(SPREAD, tenant, pair[0], "object-")?;
        let (home, member) = (cluster.partition(pair[0])?, cluster.partition(pair[1])?);
        let bucket = bucket_at(SPREAD, tenant, pair[1])?;
        let (subject, predicate, object_id) = (
            id_in(bucket, 1)?,
            id_in(bucket, 2)?,
            id_in(bucket_at(SPREAD, tenant, pair[0])?, 3)?,
        );
        name(home, tenant, &object, object_id);
        name(home, tenant, PREDICATE, predicate);
        name(member, tenant, PREDICATE, predicate);
        name(member, tenant, SUBJECT, subject);
        member.put(
            reverse_key(tenant, predicate, &EncodedTerm::id(object_id)),
            ids_list(&[subject]),
        );
        let (key, value) = referrers_of(tenant, object_id, bucket)?;
        home.put(key, value);
        let pattern = Pattern::of(None, Some(PREDICATE), Some(iri(&object)));
        let version = Version::bound_of(Epoch::new(SNAPSHOT)?);
        let named = pattern
            .read(tenant, true, ReadLevel::Snapshot)?
            .with_named_version(version)
            .with_epoch(SNAPSHOT);
        let latest_sets = |part: &crate::played_reads::played_partition::PlayedPartition| {
            part.read_scans()
                .iter()
                .filter(|scan| scan.at == ScanAt::Latest)
                .count()
        };
        let before = latest_sets(home);
        assert_eq!(answer(&cluster, &named).await?.column(0), [SUBJECT]);
        assert!(
            latest_sets(home) > before,
            "the set read at the latest version"
        );
        let fast = pattern.read(tenant, true, ReadLevel::MonotonicFast)?;
        let asked = member.read_scans().len();
        assert_eq!(answer(&cluster, &fast).await?.column(0), [SUBJECT]);
        let first = member
            .read_scans()
            .get(asked)
            .map(|scan| (scan.at, scan.epoch));
        assert_eq!(
            first.map(|(_, epoch)| epoch),
            Some(PLAYED_EPOCH),
            "at the epoch the home answered"
        );
        let unnamed = iri_at(SPREAD, tenant, pair[0], "unnamed-")?;
        let before_none = member.read_scans().len();
        let none = Pattern::of(None, Some(PREDICATE), Some(iri(&unnamed)));
        let answered = answer(&cluster, &snapshot(&none, tenant)?).await?;
        answered.ended()?;
        assert!(answered.rows().is_empty());
        assert_eq!(member.read_scans().len(), before_none, "no member asked");
        Ok(())
    })
}
