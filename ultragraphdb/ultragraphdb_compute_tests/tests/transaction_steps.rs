/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read of a read-write transaction: a Scan naming it, seeing its writes and recording what its commit validates.

use cucumber::{then, when};
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_protocol_wire::{Cancel, ErrorCode, TermTable, Write, WriteChange};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::read_steps::{edge_with, reads, until};
use crate::resume_steps::property_write;
use crate::scene::{
    KNOWS, NAME, Pattern, Scene, Transaction, compute_client, insert_of, iri, run, string,
};
use crate::{Steps, solo};

/// How long a stream stays silent for a scenario to take it as ended.
const SILENCE: std::time::Duration = std::time::Duration::from_millis(200);

/// Conflict kind Range (`PROTOCOL.md` §9.4).
const KIND_RANGE: u64 = 3;
/// Conflict kind AbsentTerm.
const KIND_ABSENT_TERM: u64 = 4;

/// A write of `<unrelated> <name> "x"`, for a transaction reading and then writing.
fn unrelated(scene: &Scene) -> TestResult<Write> {
    insert_of(None, &scene.iri_in(1, "unrelated")?, NAME, &string("x"))
}

/// Writes something unrelated in `first`, then commits: its refusal's conflict kind, `None` for a commit that holds.
async fn commit_a_write(scene: &Scene, first: &mut Transaction) -> TestResult<Option<u64>> {
    first.write(&unrelated(scene)?).await?;
    match first.try_commit().await? {
        Ok(_) => Ok(None),
        Err(refused) => {
            assert_eq!(refused.code, ErrorCode::Conflict, "{refused:?}");
            Ok(Some(refused.detail))
        }
    }
}

#[when(
    "a transaction writes `<a> <knows> <b>`, the write is answered, and the same transaction reads `<a> <knows> ?o` before committing"
)]
fn own_write(w: &mut Steps) -> TestResult {
    let scene = Scene::new(solo());
    run(async {
        let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(1, "b")?);
        let pattern = Pattern::of(Some(&a), Some(KNOWS), None);
        let mut own = scene.begin(1, 1).await?;
        own.write(&insert_of(None, &a, KNOWS, &iri(&b))?).await?;
        let read = own.read(&scene, &pattern, 0)?;
        let seen = scene.answer(&read).await?;
        let other = scene.begin(1, 2).await?;
        let not_seen = scene.answer(&other.read(&scene, &pattern, 0)?).await?;
        w.answers = vec![seen, not_seen];
        w.iris = vec![b];
        Ok(())
    })
}

#[then("the read returns `<b>`, and a read by any other transaction does not")]
fn sees_own(w: &mut Steps) -> TestResult {
    let [seen, not_seen] = w.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    seen.whole()?;
    assert_eq!(seen.column(0), w.iris);
    not_seen.whole()?;
    assert!(not_seen.rows().is_empty());
    Ok(())
}

#[when(
    "a transaction writes `<new> <p> \"x\"`, creating `<new>`, removes every object of `<a> <p> ?o`, and reads both"
)]
fn new_term(w: &mut Steps) -> TestResult {
    let scene = Scene::new(solo());
    run(async {
        let (new, a) = (scene.iri_in(1, "new")?, scene.iri_in(1, "a")?);
        scene
            .commit(1, &insert_of(None, &a, NAME, &string("y"))?)
            .await?;
        let mut own = scene.begin(1, 1).await?;
        let mut table = TermTable::new();
        let (subject, predicate) = (table.iri(&a), table.iri(NAME));
        own.write(&Write::new(
            table,
            vec![WriteChange::DeletePattern {
                graph: None,
                subject,
                predicate,
            }],
        ))
        .await?;
        own.write(&insert_of(None, &new, NAME, &string("x"))?)
            .await?;
        let of_new = Pattern::of(None, Some(NAME), None);
        let of_a = Pattern::of(Some(&a), Some(NAME), None);
        let both = scene.answer(&own.read(&scene, &of_new, 0)?).await?;
        let emptied = scene.answer(&own.read(&scene, &of_a, 0)?).await?;
        w.answers = vec![both, emptied];
        w.iris = vec![new];
        Ok(())
    })
}

#[then(
    "the read returns `\"x\"` for `<new>` with `<new>` named by its text, and nothing for `<a>`"
)]
fn names_new(w: &mut Steps) -> TestResult {
    let [both, emptied] = w.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    both.whole()?;
    assert_eq!(both.column(0), w.iris);
    assert_eq!(both.column(1), vec!["x".to_string()]);
    emptied.whole()?;
    assert!(emptied.rows().is_empty());
    Ok(())
}

#[when("a transaction scans 10,000 lists through compute")]
fn ten_thousand(w: &mut Steps) -> TestResult {
    const LISTS: u32 = 10_000;
    let scene = Scene::new(solo());
    run(async {
        for chunk in (0..LISTS).collect::<Vec<u32>>().chunks(2_000) {
            let mut table = TermTable::new();
            let predicate = table.iri(NAME);
            let object = table.intern(string("x"))?;
            let changes = chunk
                .iter()
                .map(|at| WriteChange::Insert {
                    graph: None,
                    subject: table.iri(&format!("http://ex.org/s{at}")),
                    predicate,
                    object,
                })
                .collect();
            scene.commit(1, &Write::new(table, changes)).await?;
        }
        scene.info.log.clear();
        let own = scene.begin(1, 1).await?;
        let read = own.read(&scene, &Pattern::of(None, Some(NAME), None), 0)?;
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "its read set holds one range and no list key, and it holds none of those lists in memory afterwards"
)]
fn one_range(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.rows().len(), 10_000);
    let scene = w.scene()?;
    let lists: Vec<_> = scene
        .info
        .log
        .scans()
        .into_iter()
        .filter(|scan| reads(scan, Keyspace::Forward))
        .collect();
    let [scan] = lists.as_slice() else {
        return Err(TestError::new(format!("{lists:?}")));
    };
    assert_eq!(scan.ranges.len(), 1, "one range");
    let frames = scene.info.log.versions().len();
    assert!(frames < 10_000, "lists answered by whole frames: {frames}");
    Ok(())
}

#[when(
    "transaction T1 reads `<nobody> <p> ?o` through compute, finding nothing, transaction T2 commits `<nobody> <p> \"x\"`, and T1 commits a write"
)]
fn term_nobody_holds(w: &mut Steps) -> TestResult {
    let scene = Scene::new(solo());
    run(async {
        let nobody = scene.iri_in(1, "nobody")?;
        let mut first = scene.begin(1, 1).await?;
        let read = first.read(&scene, &Pattern::of(Some(&nobody), Some(NAME), None), 0)?;
        let found = scene.answer(&read).await?;
        found.whole()?;
        assert!(found.rows().is_empty());
        scene
            .commit(1, &insert_of(None, &nobody, NAME, &string("x"))?)
            .await?;
        w.kind = commit_a_write(&scene, &mut first).await?;
        Ok(())
    })
}

#[then("T1 aborts with a conflict of kind AbsentTerm")]
fn aborts_absent(w: &mut Steps) {
    assert_eq!(w.kind, Some(KIND_ABSENT_TERM));
}

#[when(
    "transaction T1 reads `<a> <knows> ?o` through compute, finding nothing, transaction T2 commits `<a> <knows> <b>`, and T1 commits a write"
)]
fn range_insert(w: &mut Steps) -> TestResult {
    let scene = Scene::new(solo());
    run(async {
        let (a, b, c) = (
            scene.iri_in(1, "a")?,
            scene.iri_in(1, "b")?,
            scene.iri_in(1, "c")?,
        );
        scene
            .commit(1, &insert_of(None, &a, NAME, &string("y"))?)
            .await?;
        scene
            .commit(1, &insert_of(None, &c, KNOWS, &iri(&b))?)
            .await?;
        let mut first = scene.begin(1, 1).await?;
        let read = first.read(&scene, &Pattern::of(Some(&a), Some(KNOWS), None), 0)?;
        scene.answer(&read).await?.whole()?;
        scene
            .commit(1, &insert_of(None, &a, KNOWS, &iri(&b))?)
            .await?;
        w.kind = commit_a_write(&scene, &mut first).await?;
        Ok(())
    })
}

#[when(
    "a Commit cancels a read whose Scan records one more span before the Cancel reaches the partition, and another transaction wrote into that span"
)]
fn span_after_cancel(w: &mut Steps) -> TestResult {
    let scene = Scene::new(solo());
    run(async {
        let (a, b, c) = (
            scene.iri_in(1, "a")?,
            scene.iri_in(1, "b")?,
            scene.iri_in(1, "c")?,
        );
        scene
            .commit(1, &insert_of(None, &a, NAME, &string("y"))?)
            .await?;
        scene
            .commit(1, &insert_of(None, &c, KNOWS, &iri(&b))?)
            .await?;
        let mut first = scene.begin(1, 1).await?;
        let log = &scene.info.log;
        log.clear();
        // The dictionary's Scan goes on; the Scan of `<a>`'s list, the one more span, waits at the tap.
        log.pause_after(1);
        let read = first.read(&scene, &Pattern::of(Some(&a), Some(KNOWS), None), 0)?;
        let mut client = compute_client(scene.info).await?;
        client.send_only(&read).await?;
        let stream = client.last_stream();
        until("two Scans seen", || log.scans().len() >= 2).await?;
        assert_eq!(log.scans().len(), 2, "the list's Scan held");
        // The Cancel a Commit sends; compute's own Cancel of its Scan then follows the Scan held on the same connection.
        client.send_on(stream, &Cancel).await?;
        log.go_on();
        // The partition reads its connection's frames in order, so the Scan has run once compute's Cancel has gone on after it.
        until("a Cancel seen", || log.cancels() >= 1).await?;
        assert_eq!(log.cancels(), 1, "compute's Cancel of its Scan");
        scene
            .commit(1, &insert_of(None, &a, KNOWS, &iri(&b))?)
            .await?;
        w.kind = commit_a_write(&scene, &mut first).await?;
        let late = compio::time::timeout(SILENCE, client.next_frame()).await;
        assert!(late.is_err(), "a frame reached the client after its Cancel");
        Ok(())
    })
}

#[then(
    "the commit aborts with a conflict of kind Range, and no frame of that span reached the client"
)]
fn aborts_range_unseen(w: &mut Steps) {
    assert_eq!(w.kind, Some(KIND_RANGE));
}

#[then("T1 aborts with a conflict of kind Range")]
fn aborts_range(w: &mut Steps) {
    assert_eq!(w.kind, Some(KIND_RANGE));
}

#[when(
    "transaction T1 reads `?s <knows> <b>` through compute, transaction T2 commits `<c> <knows> <b>` in the same partition, and T1 commits a write"
)]
fn reverse_range(w: &mut Steps) -> TestResult {
    let scene = Scene::new(solo());
    run(async {
        let (a, b, c) = (
            scene.iri_in(1, "a")?,
            scene.iri_in(1, "b")?,
            scene.iri_in(1, "c")?,
        );
        scene
            .commit(1, &insert_of(None, &a, KNOWS, &iri(&b))?)
            .await?;
        let mut first = scene.begin(1, 1).await?;
        let read = first.read(&scene, &Pattern::of(None, Some(KNOWS), Some(iri(&b))), 0)?;
        scene.answer(&read).await?.whole()?;
        scene
            .commit(1, &insert_of(None, &c, KNOWS, &iri(&b))?)
            .await?;
        w.kind = commit_a_write(&scene, &mut first).await?;
        Ok(())
    })
}

/// `<x> <name>` of two subjects, in the store's key order.
async fn two_subjects(scene: &Scene) -> TestResult<Vec<String>> {
    let subjects = [scene.iri_in(1, "a")?, scene.iri_in(1, "z")?];
    for subject in &subjects {
        scene
            .commit(1, &insert_of(None, subject, NAME, &string("x"))?)
            .await?;
    }
    scene.in_key_order(&subjects).await
}

#[when(
    "T1 reads with limit 1 a range holding `<a>` and `<z>`, T2 commits an insert past `<a>` that T1's read did not reach, and T1 commits without resuming"
)]
fn stopped_at_limit(w: &mut Steps) -> TestResult {
    let scene = Scene::new(solo());
    run(async {
        let order = two_subjects(&scene).await?;
        let mut first = scene.begin(1, 1).await?;
        let read = first.read(&scene, &Pattern::of(None, Some(NAME), None), 1)?;
        let page = scene.answer(&read).await?;
        page.whole()?;
        assert_eq!(page.rows().len(), 1);
        assert!(!page.resume().is_empty());
        let z = order.get(1).context("the later subject")?;
        scene
            .commit(1, &insert_of(None, z, NAME, &string("past"))?)
            .await?;
        w.outcome = Some(first.try_commit().await?.is_ok());
        Ok(())
    })
}

#[then("T1 commits")]
fn t1_commits(w: &mut Steps) {
    assert_eq!(w.outcome, Some(true));
}

#[when(
    "T1 reads with limit 1, resumes to the range's end, T2 commits an insert into the resumed part, and T1 commits a write"
)]
fn resumed_records(w: &mut Steps) -> TestResult {
    let scene = Scene::new(solo());
    run(async {
        let order = two_subjects(&scene).await?;
        let mut first = scene.begin(1, 1).await?;
        let pattern = Pattern::of(None, Some(NAME), None);
        let read = first.read(&scene, &pattern, 1)?;
        let page = scene.answer(&read).await?;
        page.whole()?;
        let rest = crate::resume_steps::resumed_all(&read, page.resume());
        scene.answer(&rest).await?.whole()?;
        let z = order.get(1).context("the later subject")?;
        scene
            .commit(1, &insert_of(None, z, NAME, &string("past"))?)
            .await?;
        w.kind = commit_a_write(&scene, &mut first).await?;
        Ok(())
    })
}

#[when(
    "T1 reads the properties of the reifier `r` through compute, T2 sets a property through `r` and commits, and T1 commits a write"
)]
fn record_conflict(w: &mut Steps) -> TestResult {
    const SINCE: &str = "http://ex.org/since";
    let scene = Scene::new(solo());
    run(async {
        let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(1, "b")?);
        scene
            .commit(1, &edge_with(&a, KNOWS, &b, &[(SINCE, "2019")])?)
            .await?;
        let named = scene.snapshot(&Pattern::of(Some(&a), Some(KNOWS), None))?;
        let reifier = scene
            .answer(&named)
            .await?
            .column(1)
            .first()
            .cloned()
            .context("its reifier")?;
        let mut first = scene.begin(1, 1).await?;
        let read = first.read(&scene, &Pattern::of(Some(&reifier), None, None), 0)?;
        let properties = scene.answer(&read).await?;
        properties.whole()?;
        assert_eq!(properties.rows().len(), 1);
        scene
            .commit(1, &property_write(&reifier, SINCE, "2020")?)
            .await?;
        w.kind = commit_a_write(&scene, &mut first).await?;
        Ok(())
    })
}

#[when(
    "T1 reads with its graph unbound through compute, finding `<g1>` alone, T2 commits a statement in a graph the walk passed over, and T1 commits a write"
)]
fn graph_gap(w: &mut Steps) -> TestResult {
    const G1: &str = "http://ex.org/g1";
    const G2: &str = "http://ex.org/g2";
    let scene = Scene::new(solo());
    run(async {
        let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(1, "b")?);
        scene
            .commit(1, &insert_of(Some(G1), &a, KNOWS, &iri(&b))?)
            .await?;
        let mut first = scene.begin(1, 1).await?;
        let read = first.read(
            &scene,
            &Pattern::of(Some(&a), Some(KNOWS), None).any_graph(),
            0,
        )?;
        let found = scene.answer(&read).await?;
        found.whole()?;
        assert_eq!(found.rows().len(), 1);
        scene
            .commit(1, &insert_of(Some(G2), &b, KNOWS, &iri(&a))?)
            .await?;
        w.kind = commit_a_write(&scene, &mut first).await?;
        Ok(())
    })
}
