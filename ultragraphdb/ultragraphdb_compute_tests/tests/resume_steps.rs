/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Resume keys: reads continuing their own read, inside a list and a statement's record.

use cucumber::{then, when};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ComputeRead, ComputeReadView, ErrorCode, FrameView, Header,
    MAX_FRAME_SIZE, ReadLevel, RequestTerm, RequestTermView, ResumeKeyView, StatementTarget,
    TermTable, Write, WriteChange,
};
use ultragraphdb_tests::CLUSTER;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::read_steps::edge_with;
use crate::scene::{
    KNOWS, NAME, Pattern, Scene, Transaction, compute_client, iri, next_of, run, string,
};
use crate::{Steps, brief, four};

/// `count` short literals of `<a> <name>` in partition 1, and the read of them by `<a>`'s subject: `a`.
async fn listed(scene: &Scene, count: u32) -> TestResult<String> {
    let a = scene.iri_in(1, "a")?;
    let mut table = TermTable::new();
    let subject = table.iri(&a);
    let predicate = table.iri(NAME);
    let mut changes = Vec::with_capacity(count as usize);
    for at in 0..count {
        let object = table.intern(string(&format!("v{at}")))?;
        changes.push(WriteChange::Insert {
            graph: None,
            subject,
            predicate,
            object,
        });
    }
    scene.commit(1, &Write::new(table, changes)).await?;
    scene.info.log.clear();
    Ok(a)
}

/// The same read with `key` in its resume section.
pub(crate) fn resumed(read: &ComputeRead, key: Vec<u8>) -> ComputeRead {
    let again = ComputeRead::new(
        read.tenant(),
        read.read_only(),
        read.level(),
        read.terms().clone(),
        *read.pattern(),
        key,
    )
    .with_principal(read.principal())
    .with_limit(read.limit())
    .with_deadline_ms(read.deadline_ms())
    .with_client_limits(read.max_frame(), read.credit())
    .with_epoch(read.epoch())
    .with_partition(read.partition())
    .with_transaction(read.transaction());
    if read.named() {
        again.with_named_version(read.read_version())
    } else {
        again.with_read_version(read.read_version())
    }
}

/// The hash a resume key of `read` holds.
pub fn hash_of(read: &ComputeRead) -> TestResult<u64> {
    let frame = read.frame(
        Header::new(CLUSTER, ultragraphdb_protocol_wire::Command::ComputeRead).with_stream(1),
    )?;
    let view = FrameView::validate_client(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    Ok(ComputeReadView::from_frame(&view)?.pattern_hash())
}

#[when("a read with limit 2 matches 5 rows and is resumed twice")]
fn resumes_twice(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = listed(&scene, 5).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None))?
            .with_limit(2);
        let first = scene.answer(&read).await?;
        let second = scene.answer(&resumed(&read, first.resume())).await?;
        let third = scene.answer(&resumed(&read, second.resume())).await?;
        w.answers = vec![first, second, third];
        Ok(())
    })
}

#[then("the three answers return rows 1–2, 3–4 and 5, the last with no resume key")]
fn three_pages(w: &mut Steps) -> TestResult {
    let [first, second, third] = w.answers.as_slice() else {
        return Err(TestError::new("three answers"));
    };
    for answer in &w.answers {
        answer.whole()?;
    }
    assert_eq!(first.rows().len(), 2);
    assert_eq!(second.rows().len(), 2);
    assert_eq!(third.rows().len(), 1);
    assert!(!first.resume().is_empty());
    assert!(!second.resume().is_empty());
    assert!(third.resume().is_empty());
    let mut seen: Vec<String> = w
        .answers
        .iter()
        .flat_map(|answer| answer.column(0))
        .collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 5, "no row repeated or skipped: {seen:?}");
    Ok(())
}

#[when("a resume key from one read is used with a different pattern")]
fn another_pattern(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = listed(&scene, 3).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None))?
            .with_limit(1);
        let key = scene.answer(&read).await?.resume();
        let other = scene
            .snapshot(&Pattern::of(Some(&a), Some(KNOWS), None))?
            .with_limit(1);
        w.answers = vec![scene.answer(&resumed(&other, key)).await?];
        Ok(())
    })
}

#[then("the read is refused with an error naming the resume key")]
fn names_key(w: &mut Steps) -> TestResult {
    refused_naming_key(w)
}

fn refused_naming_key(w: &Steps) -> TestResult {
    let refused = w.answer()?.refusal()?;
    assert_eq!(refused.code, ErrorCode::Protocol, "{refused:?}");
    assert!(refused.message.contains("resume key"), "{refused:?}");
    Ok(())
}

#[when(
    "a read with limit 1 over `<a> <knows> ?o` returns `<b>` and its resume key is decoded by the server"
)]
fn decoded_key(w: &mut Steps) -> TestResult {
    key_of_knows(w)
}

/// The read of `<a> <knows> ?o` of limit 1 over two objects, into `w`.
fn key_of_knows(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let (a, b, c) = (
            scene.iri_in(1, "a")?,
            scene.iri_in(1, "b")?,
            scene.iri_in(1, "c")?,
        );
        scene.insert(&a, KNOWS, &iri(&b)).await?;
        scene.insert(&a, KNOWS, &iri(&c)).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(KNOWS), None))?
            .with_limit(1);
        let answer = scene.answer(&read).await?;
        let first = answer.column(0).first().cloned().context("a row")?;
        let id = scene.id_in(1, &first).await?;
        w.hash = hash_of(&read)?;
        w.iris = vec![first];
        w.keys = vec![id.to_be_bytes().to_vec()];
        w.answers = vec![answer];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

/// Fails unless the answer's resume key holds the expected hash and the row's text, and no system ID.
fn key_holds_terms(w: &Steps) -> TestResult {
    let bytes = w.answer()?.resume();
    let key = ResumeKeyView::read(&bytes)?;
    assert_eq!(key.pattern(), w.hash);
    let object = key.terms().get(key.row().object()).context("the object")?;
    let expected = w.iris.first().context("the row")?;
    assert_eq!(object, RequestTermView::Iri(expected));
    let id = w.keys.first().context("the ID")?;
    let holds = |needle: &[u8]| bytes.windows(needle.len()).any(|window| window == needle);
    assert!(!holds(id), "a system ID in {bytes:?}");
    Ok(())
}

#[then(
    "the key holds the pattern's hash, its `scope` and the text `http://ex.org/b`, and no system ID or store key"
)]
fn holds_terms(w: &mut Steps) -> TestResult {
    key_holds_terms(w)?;
    let bytes = w.answer()?.resume();
    let scope = ResumeKeyView::read(&bytes)?.scope();
    assert_ne!(scope, 0, "a snapshot read's key names its version");
    Ok(())
}

#[when(
    "a read stops at its limit after the row whose object is `http://ex.org/b`, and the server decodes the resume key it returned"
)]
fn stops_after_row(w: &mut Steps) -> TestResult {
    key_of_knows(w)
}

#[then(
    "the key holds the pattern's hash and the text `http://ex.org/b`, and no store key or system ID"
)]
fn identity_holds_terms(w: &mut Steps) -> TestResult {
    key_holds_terms(w)
}

#[when("a client changes one byte of a resume key and continues its read with it")]
fn tampered(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = listed(&scene, 3).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None))?
            .with_limit(1);
        let mut key = scene.answer(&read).await?.resume();
        let at = key.len() / 2;
        let byte = key.get_mut(at).context("a byte")?;
        *byte ^= 0x40;
        w.answers = vec![scene.answer(&resumed(&read, key)).await?];
        Ok(())
    })
}

#[then("the read is refused with an error naming the resume key as malformed")]
fn malformed(w: &mut Steps) -> TestResult {
    refused_naming_key(w)
}

#[when("a read matches nothing")]
fn matches_nothing(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = scene.iri_in(1, "a")?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None))?
            .with_limit(1);
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })
}

#[then("it returns no rows and no resume key")]
fn no_rows_no_key(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    assert!(answer.rows().is_empty());
    assert!(answer.resume().is_empty());
    let batch = answer.batches.first().context("a batch")?;
    assert_eq!(batch.columns, 2, "the unbound object and the reifier");
    Ok(())
}

#[when(
    "a resume key from one read-write transaction's read is used with the same pattern in another read-write transaction"
)]
fn another_transaction(w: &mut Steps) -> TestResult {
    let scene = Scene::new(crate::solo());
    run(async {
        let a = listed(&scene, 3).await?;
        let pattern = Pattern::of(Some(&a), Some(NAME), None);
        let first: Transaction = scene.begin(1, 1).await?;
        let read = first.read(&scene, &pattern, 1)?;
        let key = scene.answer(&read).await?.resume();
        let second = scene.begin(1, 2).await?;
        let other = second.read(&scene, &pattern, 1)?;
        w.answers = vec![scene.answer(&resumed(&other, key)).await?];
        Ok(())
    })
}

#[then("the read is refused with Error 1, the message naming the resume key")]
fn refused_key(w: &mut Steps) -> TestResult {
    refused_naming_key(w)
}

#[when(
    "a read-only transaction at snapshot level reads with limit 2, its connection is lost, and a new Begin names the version the first Begin answered and resumes with the key"
)]
fn reconnect(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = listed(&scene, 5).await?;
        let pattern = Pattern::of(Some(&a), Some(NAME), None);
        let first = scene.snapshot(&pattern)?.with_limit(2);
        let page = scene.answer(&first).await?;
        let named = scene
            .read_of(&pattern, true, ReadLevel::Snapshot)?
            .with_named_version(first.read_version())
            .with_limit(0);
        let rest = scene.answer(&resumed(&named, page.resume())).await?;
        w.answers = vec![page, rest];
        Ok(())
    })
}

#[then("the read returns rows 3 onward at that version, neither repeating nor skipping a row")]
fn rows_three_on(w: &mut Steps) -> TestResult {
    let [first, rest] = w.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    rest.whole()?;
    assert_eq!(rest.rows().len(), 3);
    let mut seen = first.column(0);
    seen.extend(rest.column(0));
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 5, "{seen:?}");
    Ok(())
}

#[when("a reader resumes with a key at a named version the partition has discarded")]
fn discarded(w: &mut Steps) -> TestResult {
    let scene = Scene::new(brief());
    run(async {
        let a = listed(&scene, 3).await?;
        let pattern = Pattern::of(Some(&a), Some(NAME), None);
        let first = scene.snapshot(&pattern)?.with_limit(1);
        let page = scene.answer(&first).await?;
        let named = scene
            .read_of(&pattern, true, ReadLevel::Snapshot)?
            .with_named_version(first.read_version())
            .with_limit(1);
        // The partition raises its discard version at most once a retention, on its own turn, and never past its watermark, which
        // commits move on.
        for at in 0..1_000 {
            let later = string(&format!("later{at}"));
            scene.insert(&a, KNOWS, &later).await?;
            let again = scene.answer(&resumed(&named, page.resume())).await?;
            let done = again.refused.is_some();
            w.answers = vec![again];
            if done {
                break;
            }
            compio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        Ok(())
    })
}

#[then("the read is refused with code 3")]
fn too_old(w: &mut Steps) -> TestResult {
    let refused = w.answer()?.refusal()?;
    assert_eq!(refused.code, ErrorCode::TooOld, "{refused:?}");
    Ok(())
}

#[when("`<a> <knows> <b>` is held under three statement IDs, and a read with limit 2 is resumed")]
fn three_statements(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(1, "b")?);
        for at in 0..3 {
            let write = edge_with(&a, KNOWS, &b, &[("http://ex.org/at", &format!("{at}"))])?;
            scene.commit(1, &write).await?;
        }
        scene.info.log.clear();
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(KNOWS), None))?
            .with_limit(2);
        let first = scene.answer(&read).await?;
        scene.info.log.clear();
        let second = scene.answer(&resumed(&read, first.resume())).await?;
        let object = scene.id_in(1, &b).await?;
        w.keys = vec![EncodedTerm::id(object).as_bytes().to_vec()];
        w.answers = vec![first, second];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "the first answer returns the first two statements by ascending ID, the second the third, and the partition reads none of the list's prefix"
)]
fn statements_resumed(w: &mut Steps) -> TestResult {
    let [first, second] = w.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    first.whole()?;
    second.whole()?;
    assert_eq!(first.rows().len(), 2);
    assert_eq!(second.rows().len(), 1);
    let mut reifiers = first.column(1);
    reifiers.extend(second.column(1));
    reifiers.sort();
    reifiers.dedup();
    assert_eq!(reifiers.len(), 3, "{reifiers:?}");
    let scene = w.scene()?;
    let after = scene
        .info
        .log
        .scans()
        .iter()
        .flat_map(|scan| scan.ranges.clone())
        .find(|range| !range.after.is_empty())
        .context("a range resuming in the list")?;
    assert_eq!(Some(&after.after), w.keys.first());
    Ok(())
}

#[when(
    "`<a> <knows> <b>` and `<a> <knows> <c>` are held with no statement ID, and a read with limit 1 is resumed"
)]
fn no_statement_ids(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let (a, b, c) = (
            scene.iri_in(1, "a")?,
            scene.iri_in(1, "b")?,
            scene.iri_in(1, "c")?,
        );
        scene.insert(&a, KNOWS, &iri(&b)).await?;
        scene.insert(&a, KNOWS, &iri(&c)).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(KNOWS), None))?
            .with_limit(1);
        let first = scene.answer(&read).await?;
        let found = first.column(0).first().cloned().context("a row")?;
        let id = scene.id_in(1, &found).await?;
        scene.info.log.clear();
        let second = scene.answer(&resumed(&read, first.resume())).await?;
        w.keys = vec![EncodedTerm::id(id).as_bytes().to_vec()];
        w.iris = vec![
            found,
            if first.column(0).first() == Some(&b) {
                c
            } else {
                b
            },
        ];
        w.answers = vec![second];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "the resumed Scan's `after` names `<b>`, compute drops `<b>` from what it answers, and the read returns `<c>`"
)]
fn skips_whole(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    let after = scene
        .info
        .log
        .scans()
        .iter()
        .flat_map(|scan| scan.ranges.clone())
        .find(|range| !range.after.is_empty())
        .context("a range resuming in the list")?;
    assert_eq!(Some(&after.after), w.keys.first());
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(
        answer.column(0).as_slice(),
        w.iris.get(1..).context("the rest")?
    );
    Ok(())
}

#[when(
    "a statement's record holds three properties, and a read of its reifier with limit 2 is resumed"
)]
fn record_resumed(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(1, "b")?);
        let properties = [
            ("http://ex.org/p1", "1"),
            ("http://ex.org/p2", "2"),
            ("http://ex.org/p3", "3"),
        ];
        scene
            .commit(1, &edge_with(&a, KNOWS, &b, &properties)?)
            .await?;
        let of_edge = scene.snapshot(&Pattern::of(Some(&a), Some(KNOWS), None))?;
        let reifier = scene
            .answer(&of_edge)
            .await?
            .column(1)
            .first()
            .cloned()
            .context("its reifier")?;
        let read = scene
            .snapshot(&Pattern::of(Some(&reifier), None, None))?
            .with_limit(2);
        let first = scene.answer(&read).await?;
        let second = scene.answer(&resumed(&read, first.resume())).await?;
        w.answers = vec![first, second];
        Ok(())
    })
}

#[then("the second answer returns the third property alone")]
fn third_property(w: &mut Steps) -> TestResult {
    let [first, second] = w.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    first.whole()?;
    second.whole()?;
    assert_eq!(first.rows().len(), 2);
    assert_eq!(second.rows().len(), 1);
    let mut all = first.column(0);
    all.extend(second.column(0));
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 3, "{all:?}");
    Ok(())
}

/// `at`, in `bytes` bytes of digits.
fn padded(at: u32, bytes: usize) -> String {
    let digits = at.to_string();
    "0".repeat(bytes.saturating_sub(digits.len())) + &digits
}

/// `rows` objects of `<a> <name>` in partition 1, each a literal of `bytes` bytes held by hash: `a`.
pub async fn large_list(scene: &Scene, rows: u32, bytes: usize) -> TestResult<String> {
    let a = scene.iri_in(1, "a")?;
    // Within what a write set holds.
    let per_commit = (300_000 / (bytes + 100)).max(1);
    for chunk in (0..rows).collect::<Vec<u32>>().chunks(per_commit) {
        let mut table = TermTable::new();
        let subject = table.iri(&a);
        let predicate = table.iri(NAME);
        let mut changes = Vec::with_capacity(chunk.len());
        for at in chunk {
            let object = table.intern(string(&padded(*at, bytes)))?;
            changes.push(WriteChange::Insert {
                graph: None,
                subject,
                predicate,
                object,
            });
        }
        scene.commit(1, &Write::new(table, changes)).await?;
    }
    scene.info.log.clear();
    Ok(a)
}

#[when("a read with limit 10,000 stops at its limit after several batches")]
fn long_limit(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        // Short literals in the floor's frames: several batches, from a list written in a few commits.
        let a = large_list(&scene, 10_001, 24).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None))?
            .with_limit(10_000)
            .with_client_limits(crate::scene::FLOOR, 4);
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })
}

#[then("section 6 of every batch but the last is empty, and the last holds the key")]
fn key_in_last(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    assert!(answer.batches.len() > 1, "several batches");
    assert_eq!(answer.rows().len(), 10_000);
    let (last, before) = answer.batches.split_last().context("the batches")?;
    assert!(!last.resume.is_empty());
    assert!(before.iter().all(|batch| batch.resume.is_empty()));
    Ok(())
}

/// A Write setting `predicate` `value` through the statement of `reifier`.
pub(crate) fn property_write(reifier: &str, predicate: &str, value: &str) -> TestResult<Write> {
    let mut table = TermTable::new();
    let target = StatementTarget::Reifier {
        graph: None,
        reifier: table.iri(reifier),
    };
    let predicate = table.iri(predicate);
    let value = table.intern(RequestTerm::Literal {
        lexical: value.to_string(),
        datatype: None,
    })?;
    Ok(Write::new(
        table,
        vec![WriteChange::SetProperty {
            statement: target,
            predicate,
            value,
        }],
    ))
}

/// The same read with `key` in its resume section and no limit, which runs on to the range's end.
pub(crate) fn resumed_all(read: &ComputeRead, key: Vec<u8>) -> ComputeRead {
    resumed(read, key).with_limit(0)
}

#[when("a read of two graphs stalls on its client's credit past `read_retain_ms`, and goes on")]
fn outlasts_retention(w: &mut Steps) -> TestResult {
    const GRAPHS: [&str; 2] = ["http://ex.org/g1", "http://ex.org/g2"];
    let scene = Scene::new(brief());
    run(async {
        let a = scene.iri_in(1, "a")?;
        for graph in GRAPHS {
            for at in 0..2 {
                let object = string(&padded(at, 300_000));
                scene
                    .commit(1, &crate::scene::insert_of(Some(graph), &a, NAME, &object)?)
                    .await?;
            }
        }
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None).any_graph())?
            .with_client_limits(crate::scene::FLOOR, 1);
        let mut client = compute_client(scene.info).await?;
        client.send_only(&read).await?;
        let stream = client.last_stream();
        let mut answer = crate::scene::ReadAnswer::default();
        loop {
            match next_of(&mut client).await? {
                Ok(batch) => {
                    let last = batch.last;
                    answer.batches.push(batch);
                    if last {
                        break;
                    }
                }
                Err(refused) => {
                    answer.refused = Some(refused);
                    break;
                }
            }
            // Well past what the partition retains between Scans, within what it lets a Scan stall, its watermark moving on.
            compio::time::sleep(std::time::Duration::from_millis(200)).await;
            scene.insert(&a, KNOWS, &string("later")).await?;
            client
                .send_on(stream, &ultragraphdb_protocol_wire::Credit::new(1))
                .await?;
        }
        w.answers = vec![answer];
        Ok(())
    })
}

#[then("the read reaches `LAST`, and no Scan of it is refused with code 3")]
fn kept_version(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.rows().len(), 4);
    Ok(())
}
