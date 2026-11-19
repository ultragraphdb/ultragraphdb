/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Resume keys: reads continuing their own read, inside a list and a statement's record.

use cucumber::{then, when};
use ultragraphdb_protocol_wire::ReadLevel;
use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::scene::{
    KNOWS, NAME, Pattern, Scene, compute_client, next_of, padded, run, string, until,
};
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
        let read = scene.snapshot(&Pattern::of(Some(&a), Some(NAME), None))?;
        scene.info.log.pause_after(1);
        let reading = scene.answer(&read);
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
        let latest = scene.read_of(&pattern, true, ReadLevel::Latest)?;
        scene.answer(&latest).await?.whole()?;
        let at_latest = scene.info.log.pins().len() as u64;
        let snapshot = scene.snapshot(&pattern)?;
        scene.answer(&snapshot).await?.whole()?;
        let at_snapshot = scene.info.log.pins();
        assert_eq!(at_snapshot, vec![snapshot.read_version()]);
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

#[when("a read of two graphs stalls on its client's credit past `read_retain_ms`, and goes on")]
fn outlasts_retention(w: &mut Steps) -> TestResult {
    const GRAPHS: [&str; 2] = ["http://ex.org/g1", "http://ex.org/g2"];
    let scene = Scene::new(brief()?);
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
