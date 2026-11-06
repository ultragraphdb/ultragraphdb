/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The frontend owner's scenarios, bound to `features/`: the owner on its own thread over real logs, asked from this one.

mod endpoint_stage;
mod endpoint_steps;
mod endpoint_steps_requests;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{World, then, when};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_frontend::{OwnerStart, OwnerThread, start_owner};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply, Join};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const CLUSTER: u64 = 7;
const A: &str = "tcp://a:1";
const B: &str = "tcp://b:1";
const SESSION: u128 = 11;
const NEXT_SESSION: u128 = 12;

static DIRECTORIES: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Default, World)]
struct Steps {
    refused: Option<ErrorReply>,
    ranges: Vec<(u32, u32, u32)>,
    claimed: bool,
    evicted: Vec<[u8; 16]>,
    leases: Vec<u64>,
    served: bool,
    ran: bool,
}

/// A refusal the scenario did not expect, as the test's error.
fn reason(refusal: &ErrorReply) -> TestError {
    TestError::new(refusal.message().to_string())
}

/// A directory no other scenario uses.
fn fresh() -> TestResult<PathBuf> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let directory = root.join(format!(
        "owner-{}-{}",
        std::process::id(),
        DIRECTORIES.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory)?;
    Ok(directory)
}

/// An owner of 4 partitions over `directory`, holding principals alice and bob, whose registry holds `sessions`.
async fn owner(directory: &std::path::Path, sessions: u32) -> TestResult<OwnerThread> {
    let verifier = ScramVerifier::derive("pw", b"salt", 4_096)?;
    let start = OwnerStart::new(
        MetadataConfig::new().with_sessions_max(sessions)?,
        SequencerConfig::new(1, 5)?,
        4,
    );
    Ok(start_owner(
        ServiceConfig::default(),
        CLUSTER,
        directory.to_path_buf(),
        start,
        vec![
            ("alice".to_string(), verifier.clone()),
            ("bob".to_string(), verifier),
        ],
    )
    .await?)
}

/// Each range of the placement `owner` holds: its start, end and partition.
async fn ranges(owner: &OwnerThread) -> TestResult<Vec<(u32, u32, u32)>> {
    let placement = owner
        .handle()
        .placement(0)
        .await
        .map_err(|refusal| reason(&refusal))?;
    Ok(placement
        .ranges()
        .iter()
        .map(|range| (range.start(), range.end(), range.partition()))
        .collect())
}

/// The principal holding `name` at `owner`.
async fn principal(owner: &OwnerThread, name: &str) -> TestResult<u64> {
    let (id, _) = owner
        .handle()
        .verifier(name.to_string())
        .await
        .context("the principal")?;
    Ok(id)
}

fn refused_with(w: &Steps, code: ErrorCode) -> TestResult<String> {
    let refusal = w.refused.as_ref().context("a refusal")?;
    assert_eq!(refusal.code(), code, "{}", refusal.message());
    Ok(refusal.message().to_string())
}

#[when("a frontend with `partition_count` 4 starts on a metadata group holding no placement")]
fn forms(w: &mut Steps) -> TestResult {
    let directory = fresh()?;
    Runtime::new()?.block_on(async {
        let first = owner(&directory, 8).await?;
        w.ranges = ranges(&first).await?;
        first.stopped().await?;
        let restarted = owner(&directory, 8).await?;
        assert_eq!(ranges(&restarted).await?, w.ranges, "a restart forms none");
        restarted.stopped().await?;
        Ok(())
    })
}

#[then("the metadata group holds an even map over 4 partitions, and a restart forms none")]
fn even(w: &mut Steps) {
    let size = |(start, end, _): &(u32, u32, u32)| u64::from(end - start);
    let mut per = [0u64; 5];
    for range in &w.ranges {
        per[range.2 as usize] += size(range);
    }
    assert_eq!(per[0], 0);
    assert_eq!(per[1..].iter().sum::<u64>(), 1 << 24);
    assert!(per[1..].iter().all(|held| *held == per[1]), "{per:?}");
}

#[when("one storage process holds partition 2 and another claims it")]
fn held(w: &mut Steps) -> TestResult {
    let directory = fresh()?;
    Runtime::new()?.block_on(async {
        let running = owner(&directory, 8).await?;
        let handle = running.handle();
        handle
            .join(Join::storage(A, vec![2]))
            .await
            .map_err(|refusal| reason(&refusal))?;
        w.refused = handle.join(Join::storage(B, vec![1, 2])).await.err();
        w.claimed = handle.join(Join::storage(B, vec![1])).await.is_ok();
        let members = handle.members().await.context("the members")?;
        assert_eq!(members.len(), 2, "the refused claim registered nothing");
        running.stopped().await?;
        Ok(())
    })
}

#[then(
    "the claim is refused with an error naming partition 2 and its holder, and the placement is unchanged"
)]
fn names_the_holder(w: &mut Steps) -> TestResult {
    let message = refused_with(w, ErrorCode::Protocol)?;
    assert!(
        message.contains("partition 2") && message.contains(A),
        "{message}"
    );
    assert!(w.claimed, "a partition no one holds is free to claim");
    Ok(())
}

#[when("a storage process holding partitions 1 and 2 restarts at the same addresses")]
fn restarted(w: &mut Steps) -> TestResult {
    let directory = fresh()?;
    Runtime::new()?.block_on(async {
        let running = owner(&directory, 8).await?;
        let claim = || Join::storage(A, vec![1, 2]);
        w.claimed = running.handle().join(claim()).await.is_ok();
        w.refused = running.handle().join(claim()).await.err();
        running.stopped().await?;
        Ok(())
    })
}

#[then("its claim succeeds")]
fn claim_succeeds(w: &mut Steps) {
    assert!(w.claimed && w.refused.is_none(), "{:?}", w.refused);
}

/// Registers `SESSION` for alice, then `NEXT_SESSION` in a registry holding one, which evicts the first.
async fn evict(running: &OwnerThread) -> TestResult<u64> {
    let alice = principal(running, "alice").await?;
    let handle = running.handle();
    handle
        .register_session(SESSION, alice)
        .await
        .map_err(|refusal| reason(&refusal))?;
    handle
        .check_session(SESSION, alice)
        .await
        .map_err(|refusal| reason(&refusal))?;
    handle
        .register_session(NEXT_SESSION, alice)
        .await
        .map_err(|refusal| reason(&refusal))?;
    Ok(alice)
}

#[when(
    "principal `alice` registers a session, and a connection authenticated as `bob` commits through it"
)]
fn other_principal(w: &mut Steps) -> TestResult {
    let directory = fresh()?;
    Runtime::new()?.block_on(async {
        let running = owner(&directory, 8).await?;
        let (alice, bob) = (
            principal(&running, "alice").await?,
            principal(&running, "bob").await?,
        );
        let handle = running.handle();
        handle
            .register_session(SESSION, alice)
            .await
            .map_err(|refusal| reason(&refusal))?;
        w.refused = handle.check_session(SESSION, bob).await.err();
        w.served = handle.check_session(SESSION, alice).await.is_ok();
        running.stopped().await?;
        Ok(())
    })
}

#[then(
    "the commit is refused with code 21, `detail` 2, and `alice`'s next request through the session is served"
)]
fn denied(w: &mut Steps) -> TestResult {
    let _message = refused_with(w, ErrorCode::PermissionDenied)?;
    assert_eq!(w.refused.as_ref().map(ErrorReply::detail), Some(2));
    assert!(w.served);
    Ok(())
}

#[when(
    "a session is evicted while a connection on another core holds it, and that connection commits next"
)]
fn other_core(w: &mut Steps) -> TestResult {
    let directory = fresh()?;
    Runtime::new()?.block_on(async {
        let running = owner(&directory, 1).await?;
        let alice = evict(&running).await?;
        let handle = running.handle().clone();
        #[expect(
            clippy::disallowed_methods,
            reason = "the scenario's point is a second real core asking the owner"
        )]
        let core = std::thread::spawn(move || {
            Runtime::new().map(|runtime| {
                runtime.block_on(async move { handle.check_session(SESSION, alice).await.err() })
            })
        });
        w.refused = core
            .join()
            .map_err(|_| TestError::new("the second core panicked"))??;
        running.stopped().await?;
        Ok(())
    })
}

#[then("the commit is refused with the eviction's error")]
fn eviction_error(w: &mut Steps) -> TestResult {
    let _message = refused_with(w, ErrorCode::SessionEvicted)?;
    Ok(())
}

#[when("a session is evicted and a subscriber waits for the next close")]
fn close_carries(w: &mut Steps) -> TestResult {
    let directory = fresh()?;
    Runtime::new()?.block_on(async {
        let running = owner(&directory, 1).await?;
        let _alice = evict(&running).await?;
        let handle = running.handle();
        let mut after = 0;
        // The eviction rides the first close after its sync; the epoch length bounds how many pass first.
        for _ in 0..1_000 {
            let close = handle
                .next_close(after)
                .await
                .map_err(|refusal| reason(&refusal))?;
            after = close.epoch().get();
            if !close.evictions().is_empty() {
                w.evicted = close
                    .evictions()
                    .iter()
                    .map(|session| session.to_be_bytes())
                    .collect();
                break;
            }
        }
        running.stopped().await?;
        Ok(())
    })
}

#[then("the close names the session, and its catalog and placement are the group's")]
fn names_the_session(w: &mut Steps) {
    assert_eq!(w.evicted, [SESSION.to_be_bytes()]);
}

#[when("a partition leases two blocks of one type")]
fn leases(w: &mut Steps) -> TestResult {
    let directory = fresh()?;
    Runtime::new()?.block_on(async {
        let running = owner(&directory, 8).await?;
        w.leases.push(
            running
                .handle()
                .lease(1, 1, 16)
                .await
                .map_err(|refusal| reason(&refusal))?,
        );
        w.leases.push(
            running
                .handle()
                .lease(1, 1, 16)
                .await
                .map_err(|refusal| reason(&refusal))?,
        );
        running.stopped().await?;
        let restarted = owner(&directory, 8).await?;
        w.leases.push(
            restarted
                .handle()
                .lease(1, 1, 16)
                .await
                .map_err(|refusal| reason(&refusal))?,
        );
        restarted.stopped().await?;
        Ok(())
    })
}

#[then("the blocks do not overlap, and a restart leases past them both")]
fn rising(w: &mut Steps) {
    assert!(
        w.leases.windows(2).all(|pair| pair[0] + 16 <= pair[1]),
        "{:?}",
        w.leases
    );
}

#[when("a partition asks for the close of its open epoch in a sequencer of long epochs")]
fn close_request(w: &mut Steps) -> TestResult {
    let directory = fresh()?;
    Runtime::new()?.block_on(async {
        let start = OwnerStart::new(MetadataConfig::new(), SequencerConfig::new(1, 10_000)?, 1);
        let running = start_owner(
            ServiceConfig::default(),
            CLUSTER,
            directory.clone(),
            start,
            Vec::new(),
        )
        .await?;
        let handle = running.handle();
        handle
            .close_request(1)
            .await
            .map_err(|refusal| reason(&refusal))?;
        // Half the epoch's length: the request, not the length, closes it.
        let close = compio::time::timeout(Duration::from_secs(5), handle.next_close(0))
            .await
            .map_err(|_| TestError::new("no close within half an epoch"))?;
        w.leases
            .push(close.map_err(|refusal| reason(&refusal))?.epoch().get());
        running.stopped().await?;
        Ok(())
    })
}

#[then("the close arrives well before the epoch's length")]
fn closed_early(w: &mut Steps) {
    assert_eq!(w.leases, [1]);
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
