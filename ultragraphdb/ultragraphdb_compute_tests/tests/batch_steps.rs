/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read's batches within the client's frame, and the stream's credit.

use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_common_role::{COMPUTE_WELCOME_CREDIT, RoleClient};
use ultragraphdb_protocol_wire::{Credit, ErrorCode};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::resume_steps::large_list;
use crate::scene::{FLOOR, NAME, Pattern, Scene, compute_client, next_batch, run};
use crate::{Steps, four};

/// Rows whose frames a client of the floor's `max_frame` can hold about one of.
const LARGE_ROW: usize = 300_000;

#[when("a client whose Hello names `max_frame` 524,288 reads more rows than one such frame holds")]
fn more_than_a_frame(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = large_list(&scene, 6, 200_000).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None))?
            .with_client_limits(FLOOR, 4);
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })
}

#[then("every Result frame it receives is at most 524,288 bytes, and every row arrives")]
fn within_frame(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    assert!(answer.batches.len() > 1);
    assert!(
        answer
            .batches
            .iter()
            .all(|batch| batch.size <= FLOOR as usize),
        "{:?}",
        answer
            .batches
            .iter()
            .map(|batch| batch.size)
            .collect::<Vec<_>>()
    );
    assert_eq!(answer.rows().len(), 6);
    Ok(())
}

#[when(
    "a client whose Hello names `max_frame` 524,288 reads a row holding a literal of 600,000 bytes"
)]
fn row_too_large(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = large_list(&scene, 1, 600_000).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None))?
            .with_client_limits(FLOOR, 4);
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })
}

#[then("the read is refused with code 6, `detail` 524,288")]
fn too_large(w: &mut Steps) -> TestResult {
    let refused = w.answer()?.refusal()?;
    assert_eq!(refused.code, ErrorCode::TooLarge, "{refused:?}");
    assert_eq!(refused.detail, u64::from(FLOOR), "{refused:?}");
    Ok(())
}

#[when(
    "a read stops at its limit, and its rows fill the last batch so that its resume key does not fit"
)]
fn key_does_not_fit(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = large_list(&scene, 2, LARGE_ROW).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None))?
            .with_client_limits(FLOOR, 4)
            .with_limit(1);
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })
}

#[then(
    "the stream's last batch holds `rows` 0 and the key, spending one credit, and the batch before it is not flagged `LAST`"
)]
fn key_alone(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    let [row, key] = answer.batches.as_slice() else {
        return Err(TestError::new(format!("{:?}", answer.batches.len())));
    };
    assert!(!row.last);
    assert_eq!(row.rows.len(), 1);
    assert!(row.resume.is_empty());
    assert!(key.last);
    assert!(key.rows.is_empty());
    assert!(!key.resume.is_empty());
    Ok(())
}

/// Whether a frame arrives on `client` within a time well past what a read takes to answer one.
async fn frame_arrives(client: &mut RoleClient) -> bool {
    compio::time::timeout(Duration::from_millis(150), client.next_frame())
        .await
        .is_ok()
}

#[when(
    "a client's Welcome names `initial_credit` 4, compute's Welcome to the frontend names 16, and a read's answer takes more frames"
)]
fn starting_credit(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = large_list(&scene, 8, LARGE_ROW).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None))?
            .with_client_limits(FLOOR, 4);
        let mut client = compute_client(scene.info).await?;
        assert_eq!(client.welcome().initial_credit(), COMPUTE_WELCOME_CREDIT);
        assert_ne!(
            read.credit(),
            COMPUTE_WELCOME_CREDIT,
            "two credits to tell apart"
        );
        client.send_only(&read).await?;
        let stream = client.last_stream();
        let mut received = 0u64;
        for _ in 0..4 {
            let _batch = next_batch(&mut client).await?;
            received += 1;
        }
        let beyond_credit = u64::from(frame_arrives(&mut client).await);
        client.send_on(stream, &Credit::new(1)).await?;
        let _granted = next_batch(&mut client).await?;
        let beyond_grant = u64::from(frame_arrives(&mut client).await);
        w.numbers = vec![received, beyond_credit, beyond_grant];
        Ok(())
    })
}

#[then(
    "compute sends 4 frames, and one more for each frame of the client's Credit the frontend relays"
)]
fn four_then_one(w: &mut Steps) -> TestResult {
    let [received, beyond_credit, beyond_grant] = w.numbers.as_slice() else {
        return Err(TestError::new("the counts"));
    };
    assert_eq!(*received, 4);
    assert_eq!(*beyond_credit, 0, "no frame past the credit");
    assert_eq!(*beyond_grant, 0, "one frame per grant");
    let _unused = w.numbers.first().context("the counts")?;
    Ok(())
}
