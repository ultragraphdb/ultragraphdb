/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Reverse reads and the keyspace a read takes by its bound positions.

use cucumber::{then, when};
use ultragraphdb_protocol_wire::ErrorCode;
use ultragraphdb_tool_bdd::TestResult;

use crate::scene::{KNOWS, Pattern, Scene, iri, run};
use crate::{Steps, four};

/// `<a>` in partition 1 and `<c>` in partition 3 each `knows` `<b>`, homed in partition 2: `b`.
async fn referrers(scene: &Scene) -> TestResult<(String, Vec<String>)> {
    let (a, b, c) = (
        scene.iri_in(1, "a")?,
        scene.iri_in(2, "b")?,
        scene.iri_in(3, "c")?,
    );
    scene.insert(&a, KNOWS, &iri(&b)).await?;
    scene.insert(&c, KNOWS, &iri(&b)).await?;
    // Rows come partition by partition: `<a>`'s, partition 1's, before `<c>`'s (`PROTOCOL.md` §9.1).
    let order = vec![a, c];
    scene.info.log.clear();
    Ok((b, order))
}

/// The read of who `knows` `<b>` of `referrers`, at a snapshot past them, into `w`.
fn reverse_read(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four()?);
    run(async {
        let (b, order) = referrers(&scene).await?;
        let read = scene.snapshot(&Pattern::of(None, Some(KNOWS), Some(iri(&b))))?;
        w.answers = vec![scene.answer(&read).await?];
        w.iris = order;
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[when(
    "`<b>` is homed in partition 2, `R(b)` names groups of partitions 1 and 3 only, over four partitions, and a reverse read of `<b>` runs"
)]
fn set_names_two(w: &mut Steps) -> TestResult {
    reverse_read(w)
}

#[then(
    "compute reads `R(b)` at partition 2, asks partitions 1 and 3 for their reverse entries, and sends nothing to partition 4"
)]
fn not_named(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    assert_eq!(scene.info.log.partitions(), vec![1, 2, 3]);
    w.answer()?.whole()
}

#[when(
    "`R(b)` names groups of partitions 1 and 3, partition 1 holds a matching row, partition 3 has not closed the read's epoch, and the transaction's deadline of 50 ms passes"
)]
fn lagging(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four()?);
    run(async {
        let (b, _) = referrers(&scene).await?;
        let epoch = scene.snapshot_epoch() + 2;
        scene.info.clock.hold(3, epoch - 1);
        let read = scene
            .snapshot_at(&Pattern::of(None, Some(KNOWS), Some(iri(&b))), epoch)?
            .with_deadline_ms(50);
        let answer = scene.answer(&read).await;
        scene.info.clock.release(3);
        w.answers = vec![answer?];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "the read fails with a timeout naming partition 3, and returns no row of partition 1 without partition 3's"
)]
fn timed_out(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    let refused = answer.refusal()?;
    assert_eq!(refused.code, ErrorCode::TimedOut, "{refused:?}");
    assert_eq!(refused.detail, 3, "{refused:?}");
    assert!(answer.batches.is_empty(), "{:?}", answer.batches);
    Ok(())
}
