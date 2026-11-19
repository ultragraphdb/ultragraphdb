/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Quad reads through a real compute process over real storage processes: which partitions a read asks, and what it answers.

use cucumber::{then, when};
use futures::FutureExt;
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_protocol_wire::{ReadLevel, ScanAt};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::client;
use crate::scene::{KNOWS, NAME, Pattern, Scene, iri, reads, run, string, until};
use crate::tap::ScanSeen;
use crate::{Steps, four};

/// Whether `scan` reads the dictionary alone.
pub fn reads_dictionary(scan: &ScanSeen) -> bool {
    scan.ranges
        .iter()
        .all(|range| range.first.first() == Some(&Keyspace::Dictionary.byte()))
}

#[when(
    "a read-only transaction's read of `<a> <p> ?o` runs, `<a>` homed in partition 1, `<p>` homed in partition 2, and partition 1's dictionary holds no entry of `<p>`"
)]
fn term_absent(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four()?);
    let (a, p) = (scene.iri_in(1, "a")?, scene.iri_in(2, "p")?);
    run(async {
        scene.insert(&a, NAME, &string("Ann")).await?;
        scene.info.log.clear();
        let read = client::snapshot(&scene, &Pattern::of(Some(&a), Some(&p), None))?;
        w.answers = vec![client::answer(&scene, &read).await?];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "the dictionary Scan of partition 1 answers no entry, compute sends no list Scan and nothing to partition 2, and the read answers one batch flagged `LAST` with no row"
)]
fn absent_no_row(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    assert_eq!(scene.info.log.partitions(), vec![1]);
    let scans = scene.info.log.scans_of(1);
    assert!(scans.iter().all(reads_dictionary), "{scans:?}");
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.batches.len(), 1);
    assert!(answer.rows().is_empty());
    Ok(())
}

#[when("a read of `<a> <knows> ?o` runs against a partition holding both terms")]
fn two_scans(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four()?);
    let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(1, "b")?);
    run(async {
        scene.insert(&a, KNOWS, &iri(&b)).await?;
        scene.info.log.clear();
        let read = scene.snapshot(&Pattern::of(Some(&a), Some(KNOWS), None))?;
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "compute's first Scan holds the terms' dictionary ranges, and its Scan of the list, keyed by their IDs, comes after that Scan's answer"
)]
fn terms_first(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    let scans = scene.info.log.scans();
    let first = scans.first().context("a first Scan")?;
    assert!(reads_dictionary(first), "{first:?}");
    assert_eq!(first.ranges.len(), 2, "{first:?}");
    let second = scans.get(1).context("a second Scan")?;
    assert!(reads(second, Keyspace::Forward), "{second:?}");
    w.answer()?.whole()
}

#[when(
    "a read-only transaction at latest level takes three Scans of its partition, and a commit lands between the first and the second"
)]
fn one_version(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four()?);
    let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(1, "b")?);
    run(async {
        scene.insert(&a, KNOWS, &iri(&b)).await?;
        scene.info.log.clear();
        // A bound subject alone: its terms, its predicates' list, then its lists.
        let read = scene.read_of(&Pattern::of(Some(&a), None, None), true, ReadLevel::Latest)?;
        scene.info.log.pause_after(1);
        let reading = scene.answer(&read);
        let committing = async {
            let reached = until("two Scans seen", || scene.info.log.scans().len() >= 2).await;
            let landed = scene.insert(&a, KNOWS, &iri(&a)).await;
            scene.info.log.go_on();
            reached?;
            landed
        };
        let (answer, landed) = futures::join!(reading.fuse(), committing.fuse());
        landed?;
        w.answers = vec![answer?];
        Ok(())
    })?;
    w.iris = vec![b];
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "the second and third Scans set `at` 1 and `read_version` the first Entries' `version`, and the read returns none of the commit"
)]
fn second_at_version(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    let log = &scene.info.log;
    let (_, chosen) = log.versions().first().copied().context("a first frame")?;
    let scans = log.scans();
    assert!(scans.len() >= 3, "{scans:?}");
    for scan in scans.iter().skip(1) {
        assert_eq!(scan.at, ScanAt::ReadVersion, "{scan:?}");
        assert_eq!(scan.read_version, chosen, "{scan:?}");
        assert_eq!(scan.epoch, 0, "{scan:?}");
    }
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.column(1), w.iris);
    Ok(())
}

#[when(
    "a snapshot read asks partitions 1 and 2, partition 1 has closed the read's epoch and partition 2 has not"
)]
fn waits_for_close(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four()?);
    let (a, c) = (scene.iri_in(1, "a")?, scene.iri_in(2, "c")?);
    run(async {
        scene.insert(&a, NAME, &string("Ann")).await?;
        scene.insert(&c, NAME, &string("Cid")).await?;
        scene.info.log.clear();
        let epoch = scene.snapshot_epoch() + 2;
        scene.info.clock.hold(2, epoch - 1);
        scene.closed_through(epoch, &[1]).await?;
        let read = scene.snapshot_at(&Pattern::of(None, Some(NAME), None), epoch)?;
        let reading = scene.answer(&read);
        let releasing = async {
            let reached = until("every partition reached", || {
                scene.info.log.partitions() == [1, 2, 3, 4]
            })
            .await;
            compio::time::sleep(std::time::Duration::from_millis(30)).await;
            let scans = scene.info.log.scans();
            scene.info.clock.release(2);
            reached.map(|()| scans)
        };
        let (answer, scans) = futures::join!(reading.fuse(), releasing.fuse());
        let scans = scans?;
        w.answers = vec![answer?];
        w.scans = scans;
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "compute sends each a dictionary Scan at that epoch and no list Scan until both have answered, so no list Scan of partition 1 is refused as stalled while partition 2 waits"
)]
fn no_list_before_close(w: &mut Steps) -> TestResult {
    assert!(w.scans.iter().all(reads_dictionary), "{:?}", w.scans);
    w.answer()?.whole()?;
    assert_eq!(w.answer()?.rows().len(), 2);
    Ok(())
}
