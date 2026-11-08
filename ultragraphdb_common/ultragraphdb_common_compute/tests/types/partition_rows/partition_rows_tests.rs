/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! One partition's part of a read (`PROTOCOL.md` §13.12, §13.13): its dictionary Scan resolving the pattern's terms, the version
//! its first answer chose, which every later Scan reads and a read-only read holds, and the rows it gives the merge.

use ultragraphdb_protocol_wire::{ComputeRead, ReadLevel, RequestTerm, ScanAt};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::played::played_cluster::{PlayedCluster, quiet_times, run};
use crate::played::played_data::{
    OneList, Term, iri_homed, plain, referrer_set, referrers, reverse,
};
use crate::played::played_partition::{EPOCH, version};
use crate::played::played_read::{Bound, ReadOutcome, read, read_of, snapshot_of};

/// The pattern binding `list`'s subject and predicate.
fn bound_list(list: &OneList) -> Bound {
    Bound {
        subject: Some(list.subject.iri.clone()),
        predicate: Some(list.predicate.iri.clone()),
        object: None,
    }
}

/// `read`'s answer from a cluster of one partition holding `list`, and the cluster.
async fn read_list(
    list: &OneList,
    read_of_list: &ComputeRead,
) -> TestResult<(ReadOutcome, PlayedCluster)> {
    let played = PlayedCluster::start(1, quiet_times()).await?;
    list.hold_in(played.partition(1)?);
    let mut client = played.client().await?;
    let answer = read(&mut client, read_of_list).await?;
    Ok((answer, played))
}

/// A bound subject and predicate read their one list: every object once, in the list's order.
#[test]
fn test_a_list_gives_each_object_once_in_its_order() -> TestResult {
    run(async {
        let list = OneList::new("subject", "predicate", 4)?;
        let (answer, _played) = read_list(&list, &snapshot_of(&bound_list(&list))?).await?;
        assert_eq!(answer.answered()?.column(0), list.object_iris());
        Ok(())
    })
}

/// A read's limit stops it after as many rows, each the list's next.
#[test]
fn test_a_limit_takes_the_first_objects_of_the_list() -> TestResult {
    run(async {
        let list = OneList::new("subject", "predicate", 4)?;
        let (answer, _played) =
            read_list(&list, &snapshot_of(&bound_list(&list))?.with_limit(2)).await?;
        let first: Vec<Option<String>> = list.object_iris().into_iter().take(2).collect();
        assert_eq!(answer.answered()?.column(0), first);
        Ok(())
    })
}

/// A read reaching one partition names it in every batch.
#[test]
fn test_a_read_reaching_one_partition_names_it() -> TestResult {
    run(async {
        let played = PlayedCluster::start(2, quiet_times()).await?;
        let subject = iri_homed("subject-", 2, 2)?;
        let list = OneList::new(
            subject.trim_start_matches("http://example.org/"),
            "predicate",
            2,
        )?;
        list.hold_in(played.partition(2)?);
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound_list(&list))?)
            .await?
            .answered()?;
        assert_eq!(answer.column(0), list.object_iris());
        let named: Vec<u32> = answer.batches.iter().map(|batch| batch.partition).collect();
        assert_eq!(named, vec![2; answer.batches.len()]);
        Ok(())
    })
}

/// A monotonic-fast read reaching one partition names the epoch that partition's first answer chose, above the read's floor.
#[test]
fn test_a_monotonic_fast_read_names_the_epoch_its_partition_chose() -> TestResult {
    run(async {
        let list = OneList::new("subject", "predicate", 2)?;
        let floor = EPOCH - 6;
        let asked = read_of(
            &bound_list(&list),
            true,
            ReadLevel::MonotonicFast,
            Vec::new(),
        )?
        .with_epoch(floor);
        let (answer, _played) = read_list(&list, &asked).await?;
        let epochs: Vec<u64> = answer
            .answered()?
            .batches
            .iter()
            .map(|batch| batch.epoch)
            .collect();
        assert_eq!(epochs, vec![EPOCH; epochs.len()]);
        Ok(())
    })
}

/// A Scan waiting for an epoch carries the time left until the read's deadline; one at a version chosen waits for none and
/// carries 0 (`PROTOCOL.md` §13.13, `deadline_ms`).
#[test]
fn test_only_a_scan_waiting_for_an_epoch_carries_a_deadline() -> TestResult {
    run(async {
        let list = OneList::new("subject", "predicate", 2)?;
        let asked = snapshot_of(&bound_list(&list))?.with_deadline_ms(10_000);
        let (answer, played) = read_list(&list, &asked).await?;
        drop(answer.answered()?);
        let scans = played.partition(1)?.scans();
        let (first, later) = scans.split_first().context("the read's Scans")?;
        assert_eq!(first.epoch, EPOCH);
        assert!((1..=10_000).contains(&first.deadline_ms), "{first:?}");
        assert!(!later.is_empty());
        assert!(
            later
                .iter()
                .all(|scan| scan.epoch == 0 && scan.deadline_ms == 0),
            "{later:?}"
        );
        Ok(())
    })
}

/// A read at latest level reads the version its partition's first answer chose in every Scan after it: `at` 1, that version and
/// epoch 0 (`PROTOCOL.md` §13.12).
#[test]
fn test_every_scan_after_the_first_reads_the_version_it_chose() -> TestResult {
    run(async {
        let list = OneList::new("subject", "predicate", 2)?;
        let asked = read_of(&bound_list(&list), true, ReadLevel::Latest, Vec::new())?;
        let (answer, played) = read_list(&list, &asked).await?;
        assert_eq!(answer.answered()?.column(0), list.object_iris());
        let scans = played.partition(1)?.scans();
        let (first, later) = scans.split_first().context("the read's Scans")?;
        assert_eq!(first.at, ScanAt::Latest);
        assert!(!later.is_empty());
        for scan in later {
            assert_eq!(
                (scan.at, scan.read_version, scan.epoch),
                (ScanAt::ReadVersion, version(), 0)
            );
        }
        Ok(())
    })
}

/// A read-only read whose version was chosen before it began holds the version its partition answered from that answer on
/// (`PROTOCOL.md` §11.9): a snapshot's, and a version the client named.
#[test]
fn test_a_read_only_read_of_a_version_chosen_before_it_holds_it() -> TestResult {
    run(async {
        let list = OneList::new("subject", "predicate", 2)?;
        let snapshot = snapshot_of(&bound_list(&list))?;
        let named = read_of(&bound_list(&list), true, ReadLevel::Snapshot, Vec::new())?
            .with_named_version(version());
        for asked in [snapshot, named] {
            let (answer, played) = read_list(&list, &asked).await?;
            drop(answer.answered()?);
            let pins = played.partition(1)?.pins();
            assert!(!pins.is_empty(), "{asked:?}");
            assert!(
                pins.iter().all(|pin| pin.read_version == version()),
                "{pins:?}"
            );
        }
        Ok(())
    })
}

/// A read-write transaction's read holds no version: its Bind holds it.
#[test]
fn test_a_read_write_read_holds_no_version() -> TestResult {
    run(async {
        let list = OneList::new("subject", "predicate", 2)?;
        let asked = read_of(&bound_list(&list), false, ReadLevel::Snapshot, Vec::new())?
            .with_partition(1)
            .with_transaction([9; 16])
            .with_read_version(version());
        let (answer, played) = read_list(&list, &asked).await?;
        assert_eq!(answer.answered()?.column(0), list.object_iris());
        assert_eq!(played.partition(1)?.pins(), Vec::new());
        Ok(())
    })
}

/// A reverse read reads the object's referrer set at its home and the object's list in the partition the set names, the object's
/// ID from its home (`PROTOCOL.md` §11.12): the member's dictionary Scan resolves the predicate alone.
#[test]
fn test_a_reverse_read_follows_the_referrer_set_from_the_home() -> TestResult {
    run(async {
        let played = PlayedCluster::start(2, quiet_times()).await?;
        let object = Term::resource(&iri_homed("object-", 1, 2)?, 1)?;
        let subject = Term::resource(&iri_homed("subject-", 2, 2)?, 2)?;
        let predicate = Term::resource("http://example.org/predicate", 3)?;
        let (home, member) = (played.partition(1)?, played.partition(2)?);
        object.name_in(home);
        home.put(referrers(&object), referrer_set(&[&subject])?);
        subject.name_in(member);
        predicate.name_in(member);
        member.put(reverse(&predicate, &object.id_term()), plain(&[&subject]));
        let bound = Bound {
            subject: None,
            predicate: Some(predicate.iri.clone()),
            object: Some(RequestTerm::Iri(object.iri.clone())),
        };
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
        assert_eq!(answer.column(0), vec![Some(subject.iri.clone())]);
        let gate = member
            .scans()
            .into_iter()
            .next()
            .context("the member's dictionary Scan")?;
        assert_eq!(gate.ranges.len(), 1, "{gate:?}");
        Ok(())
    })
}

/// What each case of [`test_what_a_partition_holds_that_does_not_read_refuses_the_read`] breaks in a partition placing `<s> <p>`
/// listing two objects, and the pattern it then reads.
type Breaks = fn(
    &crate::played_reads::played_partition::PlayedPartition,
    u64,
    &crate::played_reads::played_data::Placed,
) -> TestResult<crate::played_reads::read_answer::Pattern>;

/// The first cases of [`test_what_a_partition_holds_that_does_not_read_refuses_the_read`]: entries and lists that do not read.
fn broken_entries() -> [(&'static str, Breaks); 4] {
    use ultragraphdb_core_keys::{Graph, Keyspace, key_prefix};
    use ultragraphdb_store_quads::term_key;

    use crate::played_reads::played_data::{forward_key, ids_list, name_key};
    use crate::played_reads::read_answer::Pattern;

    const S: &str = "http://ex.org/s";
    const P: &str = "http://ex.org/p";
    [
        ("a term's entry", |part, tenant, _| {
            part.put(term_key(tenant, S), vec![1, 2, 3]);
            Ok(Pattern::of(Some(S), Some(P), None))
        }),
        ("an ID's entry", |part, tenant, placed| {
            let first = placed.objects.first().copied().context("an object")?;
            part.put(name_key(tenant, first), vec![0xFF, 0xFE]);
            Ok(Pattern::of(Some(S), Some(P), None))
        }),
        ("a list", |part, tenant, placed| {
            part.put(
                forward_key(tenant, placed.predicate, placed.subject),
                vec![0xFF; 3],
            );
            Ok(Pattern::of(Some(S), Some(P), None))
        }),
        ("a key", |part, tenant, placed| {
            let mut key = key_prefix(
                Keyspace::Forward,
                false,
                tenant,
                Some(Graph::Default),
                Some(placed.predicate),
            );
            key.push(0);
            part.put(key, ids_list(&placed.objects));
            Ok(Pattern::of(None, Some(P), None))
        }),
    ]
}

/// The last cases of [`test_what_a_partition_holds_that_does_not_read_refuses_the_read`]: sets, IDs, encodings and keys that do not read.
fn broken_keys() -> [(&'static str, Breaks); 4] {
    use ultragraphdb_core_keys::{Keyspace, QuadKey, key_prefix};

    use crate::played_reads::played_data::{first_bucket, forward_key, id_in, ids_list};
    use crate::played_reads::read_answer::{Pattern, iri};

    const S: &str = "http://ex.org/s";
    const P: &str = "http://ex.org/p";
    const O: &str = "http://ex.org/o1";
    [
        ("a referrer set", |part, tenant, placed| {
            let first = placed.objects.first().copied().context("an object")?;
            let referrers = QuadKey::Referrers {
                tenant,
                entity: first,
            };
            part.put(referrers.to_bytes(), vec![0xFF; 3]);
            Ok(Pattern::of(None, Some(P), Some(iri(O))))
        }),
        ("a dangling ID", |part, tenant, placed| {
            let mut objects = placed.objects.clone();
            objects.push(id_in(first_bucket(tenant), 99)?);
            part.put(
                forward_key(tenant, placed.predicate, placed.subject),
                ids_list(&objects),
            );
            Ok(Pattern::of(Some(S), Some(P), None))
        }),
        ("an object's encoding", |part, tenant, placed| {
            let mut list = ultragraphdb_protocol_wire::ObjectRecordsBuilder::new();
            list.push(&[0xEE], &[]);
            part.put(
                forward_key(tenant, placed.predicate, placed.subject),
                list.value().to_vec(),
            );
            Ok(Pattern::of(Some(S), Some(P), None))
        }),
        ("a walked key", |part, tenant, _| {
            let mut key = key_prefix(Keyspace::Forward, false, tenant, None, None);
            key.push(0);
            part.put(key, Vec::new());
            let mut pattern = Pattern::of(None, Some(P), None);
            pattern.graph = None;
            Ok(pattern)
        }),
    ]
}

/// What a partition holds that does not read refuses the read with code 17, naming it: a term's entry holding no ID, an ID's entry
/// holding no text, a list that holds no object records, a key in a list's range that is no key, a referrer set that does not
/// decode, a list naming an ID the dictionary lacks, an object whose encoding does not read, and a key under a walked keyspace
/// naming no graph.
#[test]
fn test_what_a_partition_holds_that_does_not_read_refuses_the_read() -> TestResult {
    use ultragraphdb_common_compute::read_times_of;
    use ultragraphdb_protocol_wire::ErrorCode;

    use crate::played_reads::played_cluster::PlayedCluster as ReadsCluster;
    use crate::played_reads::played_data::{first_bucket, place, tenant_over};
    use crate::played_reads::read_answer::answer_on;

    const S: &str = "http://ex.org/s";
    const P: &str = "http://ex.org/p";
    const OBJECTS: [&str; 2] = ["http://ex.org/o1", "http://ex.org/o2"];
    crate::played_reads::played_cluster::run(async {
        for (what, breaks) in broken_entries().into_iter().chain(broken_keys()) {
            let cluster = ReadsCluster::start(1, read_times_of(None, None)).await?;
            let tenant = tenant_over(1, 1)?;
            let part = cluster.partition(1)?;
            let placed = place(part, tenant, (first_bucket(tenant), 1), (S, P), &OBJECTS)?;
            let pattern = breaks(part, tenant, &placed)?;
            let read = pattern.read(tenant, true, ReadLevel::Latest)?;
            let answer = answer_on(&mut cluster.client().await?, &read).await?;
            let refused = answer.refusal().map_err(|_| {
                ultragraphdb_tool_bdd::TestError::new(format!("{what}: no refusal: {answer:?}"))
            })?;
            assert_eq!(refused.code, ErrorCode::Unavailable, "{what}: {refused:?}");
        }
        Ok(())
    })
}

/// The apart tenant's referrer set of `object` and its value, its one member the group of `subject`'s bucket.
fn referred_by(
    object: ultragraphdb_core_ids::SystemId,
    subject: ultragraphdb_core_ids::SystemId,
) -> TestResult<(Vec<u8>, Vec<u8>)> {
    let mut set = ultragraphdb_store_quads::ReferrerSet::new();
    set.set(
        ultragraphdb_core_ids::BucketGroup::of(subject.bucket()),
        std::num::NonZeroU64::MIN,
        ultragraphdb_core_ids::Epoch::new(1)?,
    );
    let mut value = Vec::new();
    set.encode(&mut value);
    let key = ultragraphdb_core_keys::QuadKey::Referrers {
        tenant: crate::apart::apart_reads::TENANT,
        entity: object,
    }
    .to_bytes();
    Ok((key, value))
}

/// A referrer set a partition answers over several frames, its key in one and the range's end in the next, is read to the last:
/// the reverse read reaches the member it names.
#[test]
fn test_a_referrer_set_answered_over_frames_is_read_to_its_last() -> TestResult {
    use std::time::Duration;

    use ultragraphdb_common_role::OpenRequests;
    use ultragraphdb_core_keys::{Graph, QuadKey};
    use ultragraphdb_core_terms::EncodedTerm;
    use ultragraphdb_protocol_wire::{MAX_FRAME_SIZE, PatternSlot, QuadPattern, TermTable};

    use crate::apart::apart_cluster::{ApartCluster, times};
    use crate::apart::apart_partition::VERSION;
    use crate::apart::apart_reads::{TENANT, answer_on, id, list_of, name};

    const S: &str = "http://ex.org/s";
    const P: &str = "http://ex.org/p";
    const O: &str = "http://ex.org/o";
    crate::apart::apart_cluster::run(async {
        let cluster = ApartCluster::start(1).await?;
        let part = cluster.partition(1)?;
        part.frame_per_key();
        let (subject, predicate, object) = (id(1)?, id(2)?, id(3)?);
        name(part, S, subject);
        name(part, P, predicate);
        name(part, O, object);
        let reverse = QuadKey::Reverse {
            tenant: TENANT,
            graph: Graph::Default,
            predicate,
            object: EncodedTerm::id(object),
        };
        part.put(reverse.to_bytes(), list_of(&[subject]));
        let epoch = VERSION >> 22;
        let (key, value) = referred_by(object, subject)?;
        part.put(key, value);
        let mut terms = TermTable::new();
        let (p, o) = (terms.iri(P), terms.iri(O));
        let pattern = QuadPattern::new(
            PatternSlot::DefaultGraph,
            PatternSlot::Unbound,
            PatternSlot::Bound(p),
            PatternSlot::Bound(o),
        );
        let read = ultragraphdb_protocol_wire::ComputeRead::new(
            TENANT,
            true,
            ReadLevel::Snapshot,
            terms,
            pattern,
            Vec::new(),
        )
        .with_read_version(VERSION)
        .with_epoch(epoch)
        .with_principal(1)
        .with_deadline_ms(10_000)
        .with_client_limits(MAX_FRAME_SIZE as u32, 4);
        let mut client = cluster
            .compute(times(Duration::from_secs(1)), OpenRequests::default())
            .await?;
        let answer = answer_on(&mut client, &read).await?;
        let rows = answer.whole()?;
        let first = rows.first().and_then(|row| row.first()).cloned().flatten();
        assert_eq!(first.as_deref(), Some(S));
        assert_eq!(rows.len(), 1);
        Ok(())
    })
}
