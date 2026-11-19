/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read at its transaction's level, the epochs compute reads at.

use cucumber::{then, when};
use ultragraphdb_protocol_wire::ReadLevel;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::client;
use crate::scene::{NAME, Pattern, Scene, closed_at, run, string};
use crate::{Steps, four};

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
    let scene = Scene::new(four()?);
    run(async {
        let a = scene.iri_in(1, "a")?;
        scene.insert(&a, NAME, &string("Ann")).await?;
        let (low, _) = lag(&scene, 2, 1, 3).await?;
        let read = client::read_at(
            &scene,
            &Pattern::of(None, Some(NAME), None),
            true,
            ReadLevel::MonotonicFast,
        )?;
        let answer = client::answer(&scene, &read).await;
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
