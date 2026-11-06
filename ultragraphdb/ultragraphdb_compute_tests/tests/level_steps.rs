/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read at its transaction's level, the epochs compute reads at.

use cucumber::{then, when};
use futures::FutureExt;
use ultragraphdb_protocol_wire::{ErrorCode, ReadLevel};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::scene::{ANSWER_WAIT, NAME, Pattern, Scene, closed_at, run, string, within};
use crate::{Steps, four};

#[when(
    "a client commits a write, receives a session token, and begins a transaction with that token whose read asks for the write"
)]
fn token_read(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = scene.iri_in(1, "a")?;
        let version = scene.insert_version(&a, NAME, &string("Ann")).await?;
        // The token's epoch is the commit's; the snapshot of a Begin carrying it is at least that.
        let epoch = ultragraphdb_core_ids::Version::from_u64(version)
            .epoch()
            .get();
        let read = scene.snapshot_at(&Pattern::of(Some(&a), Some(NAME), None), epoch)?;
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })
}

#[then("the read returns the write")]
fn returns_write(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.column(0), vec!["Ann".to_string()]);
    Ok(())
}

#[when(
    "a transaction begins at snapshot level, reads `<a>`'s statements, another transaction commits a new statement of `<a>`, and the first transaction reads `<a>`'s statements again"
)]
fn shared_snapshot(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = scene.iri_in(1, "a")?;
        scene.insert(&a, NAME, &string("Ann")).await?;
        let read = scene.snapshot(&Pattern::of(Some(&a), Some(NAME), None))?;
        let first = scene.answer(&read).await?;
        scene.insert(&a, NAME, &string("Cid")).await?;
        let second = scene.answer(&read).await?;
        w.answers = vec![first, second];
        Ok(())
    })
}

#[then("both reads return the same rows")]
fn same_rows(w: &mut Steps) -> TestResult {
    let [first, second] = w.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    first.whole()?;
    second.whole()?;
    assert_eq!(first.column(0), second.column(0));
    assert_eq!(first.column(0), vec!["Ann".to_string()]);
    Ok(())
}

/// Holds `lagging` at the epoch closed now and waits for it and for `ahead` to pass it by `gap`: the epochs each has closed.
async fn lag(scene: &Scene, lagging: u32, ahead: u32, gap: u64) -> TestResult<(u64, u64)> {
    let held = scene.info.clock.latest();
    scene.info.clock.hold(lagging, held);
    scene.closed_through(held, &[lagging]).await?;
    scene.closed_through(held + gap, &[ahead]).await?;
    let mut behind = scene.partition_client(lagging).await?;
    let low = closed_at(&mut behind, lagging).await?;
    let mut before = scene.partition_client(ahead).await?;
    Ok((low, closed_at(&mut before, ahead).await?))
}

#[when(
    "partition 2 has closed epoch 9 and partition 1 epoch 12, and a monotonic-fast read with its subject unbound runs over both"
)]
fn monotonic_over_both(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = scene.iri_in(1, "a")?;
        scene.insert(&a, NAME, &string("Ann")).await?;
        let (low, _) = lag(&scene, 2, 1, 3).await?;
        let read = scene.read_of(
            &Pattern::of(None, Some(NAME), None),
            true,
            ReadLevel::MonotonicFast,
        )?;
        let answer = scene.answer(&read).await;
        scene.info.clock.release(2);
        w.answers = vec![answer?];
        w.numbers = vec![low];
        Ok(())
    })
}

#[then("it answers at once, reading both partitions at epoch 9, the lower of their closed epochs")]
fn at_lower(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    let batch = answer.batches.first().context("a batch")?;
    assert_eq!(Some(&batch.epoch), w.numbers.first());
    Ok(())
}

#[when(
    "a monotonic-fast transaction's first read answers at epoch 9, and its second read touches only a partition whose latest closed epoch is 12"
)]
fn never_back(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = scene.iri_in(1, "a")?;
        scene.insert(&a, NAME, &string("Ann")).await?;
        let (low, _) = lag(&scene, 2, 1, 3).await?;
        let across = Pattern::of(None, Some(NAME), None);
        let read = scene.read_of(&across, true, ReadLevel::MonotonicFast)?;
        let first = scene.answer(&read).await;
        let floor = first
            .as_ref()
            .ok()
            .and_then(|answer| answer.batches.first())
            .map_or(low, |batch| batch.epoch);
        let alone = Pattern::of(Some(&a), Some(NAME), None);
        let later = scene
            .read_of(&alone, true, ReadLevel::MonotonicFast)?
            .with_epoch(floor);
        let second = scene.answer(&later).await;
        scene.info.clock.release(2);
        w.answers = vec![first?, second?];
        w.numbers = vec![floor];
        Ok(())
    })
}

#[then("the second read answers at epoch 12 without waiting")]
fn at_higher(w: &mut Steps) -> TestResult {
    let second = w.answers.get(1).context("the second answer")?;
    second.whole()?;
    let batch = second.batches.first().context("a batch")?;
    let floor = *w.numbers.first().context("the floor")?;
    assert!(batch.epoch > floor, "{} above {floor}", batch.epoch);
    Ok(())
}

#[when(
    "a read-only monotonic-fast transaction carries a token of partition 1 at epoch 20, and its read touches partition 1, closed at 21, and partition 2, closed at 18"
)]
fn below_token(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = scene.iri_in(1, "a")?;
        let version = scene.insert_version(&a, NAME, &string("Ann")).await?;
        let token = ultragraphdb_core_ids::Version::from_u64(version)
            .epoch()
            .get();
        let floor = token.max(scene.info.clock.latest()) + 2;
        scene.info.clock.hold(2, floor - 2);
        scene.closed_through(floor + 1, &[1]).await?;
        let read = scene
            .read_of(
                &Pattern::of(None, Some(NAME), None),
                true,
                ReadLevel::MonotonicFast,
            )?
            .with_epoch(floor);
        let reading = scene.answer(&read);
        // Released once compute's Scan of partition 2 waits at the floor, so the read cannot have chosen its epoch after.
        let releasing = async {
            let waited = within(
                ANSWER_WAIT,
                "compute's Scan of partition 2 at the floor",
                async {
                    let at_floor = || {
                        scene
                            .info
                            .log
                            .scans()
                            .iter()
                            .any(|scan| scan.partition == 2 && scan.epoch == floor)
                    };
                    while !at_floor() {
                        compio::time::sleep(std::time::Duration::from_millis(1)).await;
                    }
                    Ok(())
                },
            )
            .await;
            scene.info.clock.release(2);
            waited
        };
        let (answer, waited) = futures::join!(reading.fuse(), releasing.fuse());
        waited?;
        w.answers = vec![answer?];
        w.numbers = vec![floor];
        Ok(())
    })
}

#[then(
    "the read waits until partition 2 has closed epoch 20, then answers at epoch 20 and returns the token's commit"
)]
fn at_token(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    let batch = answer.batches.first().context("a batch")?;
    assert_eq!(Some(&batch.epoch), w.numbers.first());
    assert_eq!(answer.column(0).len(), 1);
    Ok(())
}

#[when("a transaction at latest-state level reads with its subject unbound over two partitions")]
fn latest_across(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let read = scene.read_of(
            &Pattern::of(None, Some(NAME), None),
            true,
            ReadLevel::Latest,
        )?;
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })
}

#[then("the read is refused with an error naming the level and the two partitions")]
fn level_refused(w: &mut Steps) -> TestResult {
    let refused = w.answer()?.refusal()?;
    assert_eq!(refused.code, ErrorCode::LevelRefused, "{refused:?}");
    assert!(refused.message.contains("latest"), "{refused:?}");
    assert!(refused.message.contains("[1, 2"), "{refused:?}");
    assert_eq!(refused.detail, 1 | (2 << 32), "{refused:?}");
    Ok(())
}
