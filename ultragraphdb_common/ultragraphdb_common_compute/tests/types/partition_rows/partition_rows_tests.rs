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
