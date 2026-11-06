/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Quad reads through a real compute process over real storage processes: which partitions a read asks, and what it answers.

use cucumber::{then, when};
use futures::FutureExt;
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_core_terms::{FLOAT32_VECTOR, format_vector};
use ultragraphdb_protocol_wire::{ReadLevel, RequestTerm, ScanAt, TermTable, Write, WriteChange};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::scene::{KNOWS, NAME, Pattern, Scene, answer_on, compute_client, iri, run, string};
use crate::tap::ScanSeen;
use crate::{Steps, four};

/// Waits, within a time no scenario takes, for `holds`.
pub async fn until(holds: impl Fn() -> bool) {
    for _ in 0..3_000 {
        if holds() {
            return;
        }
        compio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
}

/// Whether `scan` reads the dictionary alone.
pub fn reads_dictionary(scan: &ScanSeen) -> bool {
    scan.ranges
        .iter()
        .all(|range| range.first.first() == Some(&Keyspace::Dictionary.byte()))
}

/// Whether `scan` holds a range of `keyspace`.
pub fn reads(scan: &ScanSeen, keyspace: Keyspace) -> bool {
    scan.ranges
        .iter()
        .any(|range| range.first.first() == Some(&keyspace.byte()))
}

/// The term of a `float32vector` literal.
fn vector(elements: &[f32]) -> RequestTerm {
    RequestTerm::Literal {
        lexical: format_vector(elements.iter().copied()),
        datatype: Some(FLOAT32_VECTOR.to_string()),
    }
}

/// A Write inserting an edge with its properties.
pub fn edge_with(
    subject: &str,
    predicate: &str,
    object: &str,
    properties: &[(&str, &str)],
) -> TestResult<Write> {
    let mut table = TermTable::new();
    let subject = table.iri(subject);
    let predicate = table.iri(predicate);
    let object = table.iri(object);
    let properties = properties
        .iter()
        .map(|(key, value)| {
            let key = table.iri(key);
            let value = table.intern(string(value))?;
            Ok((key, value))
        })
        .collect::<TestResult<Vec<_>>>()?;
    Ok(Write::new(
        table,
        vec![WriteChange::InsertEdge {
            graph: None,
            subject,
            predicate,
            object,
            properties,
        }],
    ))
}

#[when("a read binds its subject to `http://ex.org/a` over two partitions")]
fn bound_subject(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(2, "b")?);
    run(async {
        scene.insert(&a, KNOWS, &iri(&b)).await?;
        let read = scene.snapshot(&Pattern::of(Some(&a), Some(KNOWS), None))?;
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })?;
    w.iris = vec![a, b];
    w.scene = Some(scene);
    Ok(())
}

#[then("compute asks only the partition of `http://ex.org/a`'s bucket, and returns its rows")]
fn asks_one(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    let [a, b] = w.iris.as_slice() else {
        return Err(TestError::new("the terms"));
    };
    assert_eq!(scene.info.log.partitions(), vec![scene.owner(a)?]);
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.column(0), vec![b.clone()]);
    let batch = answer.batches.first().context("a batch")?;
    assert_eq!(
        batch.partition,
        scene.owner(a)?,
        "the partition it reached alone"
    );
    Ok(())
}

#[when(
    "`<a>` in partition 1 and `<c>` in partition 2 each hold `name` `\"Ann\"`, and a read binds its object to `\"Ann\"` and its predicate to `name`"
)]
fn literal_object(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    let (a, c) = (scene.iri_in(1, "a")?, scene.iri_in(2, "c")?);
    run(async {
        scene.insert(&a, NAME, &string("Ann")).await?;
        scene.insert(&c, NAME, &string("Ann")).await?;
        let read = scene.snapshot(&Pattern::of(None, Some(NAME), Some(string("Ann"))))?;
        w.answers = vec![scene.answer(&read).await?];
        w.iris = scene.in_key_order(&[a, c]).await?;
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "compute asks every partition of the tenant's run, reads no referrer set, and returns `<a>` and `<c>` merged in key order"
)]
fn fanned_out(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    assert_eq!(scene.info.log.partitions(), vec![1, 2, 3, 4]);
    let batch = w.answer()?.batches.first().context("a batch")?;
    assert_eq!(batch.partition, 0, "it reached several");
    let scans = scene.info.log.scans();
    assert!(
        !scans.iter().any(|scan| reads(scan, Keyspace::Referrers)),
        "{scans:?}"
    );
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.column(0), w.iris);
    Ok(())
}

#[when(
    "a read leaves its subject and object unbound over two partitions that each hold matching rows"
)]
fn unbound_subject(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    let (a, c) = (scene.iri_in(1, "a")?, scene.iri_in(2, "c")?);
    run(async {
        scene.insert(&a, NAME, &string("Ann")).await?;
        scene.insert(&c, NAME, &string("Cid")).await?;
        let read = scene.snapshot(&Pattern::of(None, Some(NAME), None))?;
        w.answers = vec![scene.answer(&read).await?];
        w.iris = scene.in_key_order(&[a, c]).await?;
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then("compute returns the rows of both partitions merged in key order")]
fn merged(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.column(0), w.iris);
    Ok(())
}

#[when(
    "`<a>` in partition 1 and `<c>` in partition 2 each hold `embedding` `v`, `<d>` holds another vector, and a read binds its object to `v` and its predicate to `embedding`"
)]
fn vector_object(w: &mut Steps) -> TestResult {
    const EMBEDDING: &str = "http://ex.org/embedding";
    let scene = Scene::new(four());
    let (a, c, d) = (
        scene.iri_in(1, "a")?,
        scene.iri_in(2, "c")?,
        scene.iri_in(3, "d")?,
    );
    let v = vector(&[1.0, 2.0]);
    run(async {
        scene.insert(&a, EMBEDDING, &v).await?;
        scene.insert(&c, EMBEDDING, &v).await?;
        scene.insert(&d, EMBEDDING, &vector(&[3.0, 4.0])).await?;
        let read = scene.snapshot(&Pattern::of(None, Some(EMBEDDING), Some(v.clone())))?;
        w.answers = vec![scene.answer(&read).await?];
        w.iris = scene.in_key_order(&[a, c]).await?;
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "compute asks every partition of the tenant's run for the forward lists of `embedding`, and returns `<a>` and `<c>` alone"
)]
fn vector_rows(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    assert_eq!(scene.info.log.partitions(), vec![1, 2, 3, 4]);
    // Partition 4 holds no statement of the predicate: its dictionary says so, and no list is read there.
    for partition in 1..=3 {
        let scans = scene.info.log.scans_of(partition);
        assert!(
            scans.iter().any(|scan| reads(scan, Keyspace::Forward)),
            "{scans:?}"
        );
    }
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.column(0), w.iris);
    Ok(())
}

#[when(
    "`<a> <knows> <b>` carries `<since> 2020` under the reifier `r`, and a read binds its subject to `r`"
)]
fn reifier_subject(w: &mut Steps) -> TestResult {
    const SINCE: &str = "http://ex.org/since";
    let scene = Scene::new(four());
    let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(2, "b")?);
    run(async {
        let write = edge_with(&a, KNOWS, &b, &[(SINCE, "2020")])?;
        scene.commit(1, &write).await?;
        let read = scene.snapshot(&Pattern::of(Some(&a), Some(KNOWS), None))?;
        let named = scene.answer(&read).await?;
        named.whole()?;
        let reifier = named.column(1).first().cloned().context("its reifier")?;
        scene.info.log.clear();
        let of_reifier = scene.snapshot(&Pattern::of(Some(&reifier), None, None))?;
        w.answers = vec![scene.answer(&of_reifier).await?];
        w.iris = vec![reifier, SINCE.to_string()];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "compute reads the statement's record in the statements keyspace of the partition `r`'s bucket names, and returns one row for `<since>` and `2020` with `ABSENT` in the reifier column"
)]
fn reifier_rows(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    let [reifier, since] = w.iris.as_slice() else {
        return Err(TestError::new("the terms"));
    };
    let owner = scene.owner(reifier)?;
    assert_eq!(scene.info.log.partitions(), vec![owner]);
    assert!(
        scene
            .info
            .log
            .scans()
            .iter()
            .any(|scan| reads(scan, Keyspace::Statements))
    );
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.column(0), vec![since.clone()]);
    assert_eq!(answer.column(1), vec!["2020".to_string()]);
    assert_eq!(
        answer.rows().first().and_then(|row| row.get(2).cloned()),
        Some(None)
    );
    Ok(())
}

#[when(
    "a read-only transaction's read of `<a> <p> ?o` runs, `<a>` homed in partition 1, `<p>` homed in partition 2, and partition 1's dictionary holds no entry of `<p>`"
)]
fn term_absent(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    let (a, p) = (scene.iri_in(1, "a")?, scene.iri_in(2, "p")?);
    run(async {
        scene.insert(&a, NAME, &string("Ann")).await?;
        scene.info.log.clear();
        let read = scene.snapshot(&Pattern::of(Some(&a), Some(&p), None))?;
        w.answers = vec![scene.answer(&read).await?];
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
    let scene = Scene::new(four());
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
    let scene = Scene::new(four());
    let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(1, "b")?);
    run(async {
        scene.insert(&a, KNOWS, &iri(&b)).await?;
        scene.info.log.clear();
        // A bound subject alone: its terms, its predicates' list, then its lists.
        let read = scene.read_of(&Pattern::of(Some(&a), None, None), true, ReadLevel::Latest)?;
        scene.info.log.pause_after(1);
        let reading = scene.answer(&read);
        let committing = async {
            until(|| scene.info.log.scans().len() >= 2).await;
            let landed = scene.insert(&a, KNOWS, &iri(&a)).await;
            scene.info.log.go_on();
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
    let scene = Scene::new(four());
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
            until(|| scene.info.log.partitions() == [1, 2, 3, 4]).await;
            compio::time::sleep(std::time::Duration::from_millis(30)).await;
            let scans = scene.info.log.scans();
            scene.info.clock.release(2);
            scans
        };
        let (answer, scans) = futures::join!(reading.fuse(), releasing.fuse());
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

#[when("a read fans out over three partitions")]
fn fan_out_on_core(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    let subjects = [
        scene.iri_in(1, "a")?,
        scene.iri_in(2, "b")?,
        scene.iri_in(3, "c")?,
    ];
    run(async {
        for subject in &subjects {
            scene.insert(subject, NAME, &string("Ann")).await?;
        }
        scene.info.log.clear();
        let read = scene.snapshot(&Pattern::of(None, Some(NAME), None))?;
        let mut client = compute_client(scene.info).await?;
        let first = answer_on(&mut client, &read).await?;
        let connections: Vec<usize> = scene
            .info
            .log
            .scans()
            .iter()
            .map(|scan| scan.connection)
            .collect();
        scene.info.log.clear();
        let second = answer_on(&mut client, &read).await?;
        let again: Vec<usize> = scene
            .info
            .log
            .scans()
            .iter()
            .map(|scan| scan.connection)
            .collect();
        w.answers = vec![first, second];
        w.connections = vec![connections, again];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then("its Scans, its merge and its Result frames all run on the core its ComputeRead arrived on")]
fn stays_on_core(w: &mut Steps) -> TestResult {
    let distinct = |list: &[usize]| {
        let mut sorted = list.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        sorted
    };
    let [first, second] = w.connections.as_slice() else {
        return Err(TestError::new("the connections"));
    };
    // One connection per partition of the read's core: each core dials its own, and a read on the same connection stays on it.
    assert_eq!(distinct(first).len(), 4, "{first:?}");
    assert_eq!(distinct(first), distinct(second));
    for answer in &w.answers {
        answer.whole()?;
        assert_eq!(answer.rows().len(), 3);
    }
    Ok(())
}
