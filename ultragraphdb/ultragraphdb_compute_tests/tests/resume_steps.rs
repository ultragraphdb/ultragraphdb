/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Resume keys: reads continuing their own read, inside a list and a statement's record.

use cucumber::{then, when};
use ultragraphdb_protocol_wire::{Credit, DEFAULT_INITIAL_CREDIT, ReadLevel};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::client::{self, EndpointClient, next_of};
use crate::scene::{FLOOR, KNOWS, NAME, Pattern, ReadAnswer, Scene, padded, run, string, until};
use crate::{Steps, brief, four};

/// How long a late read's list Scan is held back: past what the brief cluster retains, within what it lets a Scan stall.
const HELD_PAST_RETENTION: std::time::Duration = std::time::Duration::from_millis(200);

#[when(
    "a read at snapshot level has its dictionary Scan answered, and its list Scan arrives past `read_retain_ms` while later epochs close"
)]
fn late_list_scan(w: &mut Steps) -> TestResult {
    let scene = Scene::new(brief()?);
    run(async {
        let a = scene.iri_in(1, "a")?;
        scene.insert(&a, NAME, &string("kept")).await?;
        let read = client::snapshot(&scene, &Pattern::of(Some(&a), Some(NAME), None)).await?;
        scene.info.log.pause_after(1);
        let reading = client::answer(&scene, &read);
        let moving_on = async {
            until("the list's Scan seen", || scene.info.log.scans().len() >= 2).await?;
            // Each commit moves the watermark past the version, and each close the discard version towards it.
            for _ in 0..4 {
                compio::time::sleep(HELD_PAST_RETENTION / 4).await;
                scene.insert(&a, KNOWS, &string("later")).await?;
            }
            scene.info.log.go_on();
            Ok::<(), TestError>(())
        };
        let (answer, moved) = futures::join!(reading, moving_on);
        moved?;
        w.answers = vec![answer?];
        Ok(())
    })
}

#[then("the read reaches `LAST` with its row, and no Scan of it is refused with code 3")]
fn late_read_answered(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.rows().len(), 1);
    Ok(())
}

#[when(
    "a read at latest level and a read at snapshot level each answer within a quarter of `read_retain_ms`"
)]
fn short_reads(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four()?);
    run(async {
        let a = scene.iri_in(1, "a")?;
        scene.insert(&a, NAME, &string("x")).await?;
        let pattern = Pattern::of(Some(&a), Some(NAME), None);
        scene.info.log.clear();
        let latest = client::read_at(&scene, &pattern, true, ReadLevel::Latest)?;
        client::answer(&scene, &latest).await?.whole()?;
        let at_latest = scene.info.log.pins().len() as u64;
        let snapshot = client::snapshot(&scene, &pattern).await?;
        client::answer(&scene, &snapshot).await?.whole()?;
        let at_snapshot = scene.info.log.pins();
        assert_eq!(at_snapshot, vec![snapshot.begin.read_version()]);
        w.numbers = vec![at_latest, at_snapshot.len() as u64];
        Ok(())
    })
}

#[then(
    "the read at snapshot level opened one Scan holding its version, and the read at latest level none"
)]
fn pins_by_level(w: &mut Steps) {
    assert_eq!(w.numbers, vec![0, 1]);
}

/// The rows each graph holds: as many as a client's starting credit, so the first graph's batches spend it, a row a batch at
/// the floor's frame, and the read stalls before the second graph's.
const ROWS_PER_GRAPH: u32 = DEFAULT_INITIAL_CREDIT;

#[when("a read of two graphs stalls on its client's credit past `read_retain_ms`, and goes on")]
fn outlasts_retention(w: &mut Steps) -> TestResult {
    const GRAPHS: [&str; 2] = ["http://ex.org/g1", "http://ex.org/g2"];
    let scene = Scene::new(brief()?);
    run(async {
        let a = scene.iri_in(1, "a")?;
        for graph in GRAPHS {
            for at in 0..ROWS_PER_GRAPH {
                let object = string(&padded(at, 300_000));
                scene
                    .commit(1, &crate::scene::insert_of(Some(graph), &a, NAME, &object)?)
                    .await?;
            }
        }
        let mut read =
            client::snapshot(&scene, &Pattern::of(Some(&a), Some(NAME), None).any_graph()).await?;
        read.max_frame = FLOOR;
        let mut client = EndpointClient::connect(scene.info, read.max_frame).await?;
        let transaction = match client.begin(&read.begin).await? {
            Ok(transaction) => transaction,
            Err(refused) => {
                return Err(TestError::new(format!(
                    "the Begin was refused: {refused:?}"
                )));
            }
        };
        let stream = client.send_read(transaction, &read).await?;
        let mut credit = DEFAULT_INITIAL_CREDIT;
        let mut stalled = false;
        let mut answer = ReadAnswer::default();
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
            credit = credit.checked_sub(1).context("a batch past the credit")?;
            if credit == 0 {
                // Well past what the partition retains between Scans, within what it lets a Scan stall, its watermark moving on.
                compio::time::sleep(std::time::Duration::from_millis(200)).await;
                scene.insert(&a, KNOWS, &string("later")).await?;
                client
                    .send_on(stream, &Credit::new(DEFAULT_INITIAL_CREDIT))
                    .await?;
                credit = DEFAULT_INITIAL_CREDIT;
                stalled = true;
            }
        }
        // A refusal is the Then's to report.
        if !stalled && answer.refused.is_none() {
            return Err(TestError::new("the read never stalled on its credit"));
        }
        w.answers = vec![answer];
        Ok(())
    })?;
    w.numbers = vec![GRAPHS.len() as u64 * u64::from(ROWS_PER_GRAPH)];
    Ok(())
}

#[then("the read reaches `LAST`, and no Scan of it is refused with code 3")]
fn kept_version(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(vec![answer.rows().len() as u64], w.numbers);
    Ok(())
}
