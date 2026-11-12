/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a partition retains, through real frames and a short `read_retain_ms`: epochs close as the frontend closes them, so a test
//! waits in milliseconds what the specification's scenarios wait in seconds.

use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_common_role::{PARTITION_INITIAL_CREDIT, RoleClient};
use ultragraphdb_core_ids::{Epoch, Version};
use ultragraphdb_protocol_wire::{BindView, BodyView, Credit, ErrorCode, ScanAt, ScanRange};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::read_kernel_steps::{
    Frame, PARTITION, PRINCIPAL, Refused, alone, answer, bind_of, dial, list_ranges, next_frame,
    of_transaction, partition_holding, plain, run,
};
use crate::scan_ranges_steps::{commit_later, committed_and_closed, scan_at};
use ultragraphdb_protocol_wire::EntriesSequence;

/// The `read_retain_ms` these scenarios set.
pub(crate) const RETAIN_MS: u64 = 200;

/// How long a test waits for the discard version to pass a version, in looks a tenth of the retention apart.
const LOOKS: u32 = 200;

fn retaining(w: &mut Steps) -> TestResult {
    partition_holding(w, &[("storage.read_retain_ms", &RETAIN_MS.to_string())])
}

fn retention_part(parts: u64) -> Duration {
    Duration::from_millis(RETAIN_MS * parts / 10)
}

/// The Scan of `version` of `epoch`: its frames' version, or its refusal.
async fn read_at(
    client: &mut RoleClient,
    version: u64,
    epoch: u64,
) -> TestResult<Result<u64, Refused>> {
    let (batches, refused) = answer(client, &scan_at(version, epoch), false).await?;
    match refused {
        Some(refused) => Ok(Err(refused)),
        None => Ok(Ok(batches.first().context("a frame")?.version)),
    }
}

/// Scans at `version` until a read of it is refused: that refusal.
pub(crate) async fn refused_once_discarded(
    client: &mut RoleClient,
    version: u64,
    epoch: u64,
) -> TestResult<Refused> {
    for look in 0..u64::from(LOOKS) {
        if let Err(refused) = read_at(client, version, epoch).await? {
            return Ok(refused);
        }
        let _later = commit_later(look).await?;
        compio::time::sleep(retention_part(1)).await;
    }
    Err(TestError::new(format!("version {version} stayed readable")))
}

#[when(
    "a read-only transaction at snapshot level reads at epoch 20, epochs close for 30 s with `read_retain_ms` 60,000, and it reads again"
)]
fn back_within(w: &mut Steps) -> TestResult {
    retaining(w)?;
    run(async {
        let mut client = dial().await?;
        let epoch = committed_and_closed(&mut client).await?;
        let version = Version::bound_of(Epoch::new(epoch)?);
        let first = read_at(&mut client, version, epoch).await?;
        assert_eq!(first.as_ref().ok(), Some(&version), "{first:?}");
        let _later = commit_later(1).await?;
        compio::time::sleep(retention_part(3)).await;
        let again = read_at(&mut client, version, epoch).await?;
        w.kernel.numbers = vec![version, again.as_ref().copied().unwrap_or(0)];
        Ok(())
    })
}

#[then("the second read answers at epoch 20's version bound")]
fn answered_at_bound(w: &mut Steps) -> TestResult {
    let [version, again] = w.kernel.numbers.as_slice() else {
        return Err(TestError::new("the versions"));
    };
    assert_eq!(version, again);
    w.roles = None;
    Ok(())
}

#[when(
    "a read-only transaction at snapshot level reads at epoch 20, the partition raises its discard version past that version bound once a later epoch has been closed for more than `read_retain_ms`, and it reads again"
)]
fn back_past(w: &mut Steps) -> TestResult {
    retaining(w)?;
    run(async {
        let mut client = dial().await?;
        let epoch = committed_and_closed(&mut client).await?;
        let version = Version::bound_of(Epoch::new(epoch)?);
        let first = read_at(&mut client, version, epoch).await?;
        assert!(first.is_ok(), "{first:?}");
        let refused = refused_once_discarded(&mut client, version, epoch).await?;
        w.kernel.refused = vec![refused];
        Ok(())
    })
}

#[then("the read is refused with code 3")]
fn read_too_old(w: &mut Steps) -> TestResult {
    let refused = w.kernel.refused.first().context("the refusal")?;
    assert_eq!(refused.code, ErrorCode::TooOld, "{refused:?}");
    w.roles = None;
    Ok(())
}

/// Runs a Scan at a closed epoch's bound past the retention: its frames, with later commits closing later epochs meanwhile; a read
/// of that version in the middle of it; and the version.
async fn long_scan(client: &mut RoleClient) -> TestResult<(u64, u64, Vec<Result<u64, Refused>>)> {
    let epoch = committed_and_closed(client).await?;
    let version = Version::bound_of(Epoch::new(epoch)?);
    let ranges = list_ranges(PARTITION_INITIAL_CREDIT + 1);
    let scan = plain(ScanAt::ReadVersion, version, epoch, 30_000, &ranges);
    client.send_only(&scan).await?;
    let stream = client.last_stream();
    let mut sequence = EntriesSequence::new(scan.ranges());
    let mut versions = Vec::new();
    for _ in 0..PARTITION_INITIAL_CREDIT {
        if let Frame::Entries(batch) = next_frame(client, &mut sequence).await? {
            versions.push(Ok(batch.version));
        }
    }
    for tag in 0..3 {
        let _later = commit_later(tag).await?;
        compio::time::sleep(retention_part(10)).await;
    }
    versions.push(read_at(&mut dial().await?, version, epoch).await?);
    client.send_on(stream, &Credit::new(1)).await?;
    match next_frame(client, &mut sequence).await? {
        Frame::Entries(batch) => {
            assert!(batch.last, "the last frame");
            versions.push(Ok(batch.version));
        }
        Frame::Refused(refused) => versions.push(Err(refused)),
        Frame::Other(command) => {
            return Err(TestError::new(format!("command {}", command.number())));
        }
    }
    Ok((version, epoch, versions))
}

#[when("a Scan keeps taking frames for longer than `read_retain_ms` while later epochs close")]
fn open_scan(w: &mut Steps) -> TestResult {
    retaining(w)?;
    run(async {
        let (version, _, versions) = long_scan(&mut dial().await?).await?;
        w.kernel.numbers = vec![version];
        for held in versions {
            match held {
                Ok(read) => w.kernel.numbers.push(read),
                Err(refused) => w.kernel.refused.push(refused),
            }
        }
        Ok(())
    })
}

#[then(
    "the discard version stays at or below the Scan's version, and the Scan reaches `LAST` at that version"
)]
fn scan_held(w: &mut Steps) -> TestResult {
    assert!(w.kernel.refused.is_empty(), "{:?}", w.kernel.refused);
    let (version, read) = w.kernel.numbers.split_first().context("the version")?;
    assert_eq!(u32::try_from(read.len())?, PARTITION_INITIAL_CREDIT + 2);
    assert!(read.iter().all(|at| at == version), "{read:?}");
    w.roles = None;
    Ok(())
}

#[when(
    "a Scan at epoch 20's version bound stays open for 90 s with `read_retain_ms` 60,000 while later epochs close, it ends, the partition raises its discard version past that bound, and a next Scan at that version arrives"
)]
fn scan_then_next(w: &mut Steps) -> TestResult {
    retaining(w)?;
    run(async {
        let mut client = dial().await?;
        let (version, epoch, versions) = long_scan(&mut client).await?;
        assert!(versions.iter().all(Result::is_ok), "{versions:?}");
        let refused = refused_once_discarded(&mut client, version, epoch).await?;
        w.kernel.refused = vec![refused];
        Ok(())
    })
}

#[then("the next Scan is refused with code 3")]
fn next_too_old(w: &mut Steps) -> TestResult {
    let refused = w.kernel.refused.first().context("the refusal")?;
    assert_eq!(refused.code, ErrorCode::TooOld, "{refused:?}");
    w.roles = None;
    Ok(())
}

#[when(
    "a read-write transaction bound at a version is open while epochs close for longer than `read_retain_ms`"
)]
fn transaction_open(w: &mut Steps) -> TestResult {
    retaining(w)?;
    run(async {
        let mut client = dial().await?;
        let _closed = committed_and_closed(&mut client).await?;
        let reply = client.request(&bind_of(50, PRINCIPAL)).await?;
        let read = BindView::from_frame(&reply.view()?)?.read_version();
        drop(reply);
        let bind_stream = client.last_stream();
        for tag in 0..3 {
            let _later = commit_later(tag).await?;
            compio::time::sleep(retention_part(10)).await;
        }
        let mut scan = of_transaction(50, read, PRINCIPAL);
        let (first, past) = alone(ultragraphdb_core_keys::key_prefix(
            ultragraphdb_core_keys::Keyspace::Forward,
            false,
            crate::TENANT,
            None,
            None,
        ));
        scan.push_range(ScanRange::new(PARTITION, &first, &past));
        let (batches, refused) = answer(&mut client, &scan, false).await?;
        w.kernel.batches = batches;
        w.kernel.refused.extend(refused);
        client
            .send_on(bind_stream, &ultragraphdb_protocol_wire::Abort)
            .await?;
        // Ended, the transaction holds nothing: the version goes as any other.
        let epoch = Version::from_u64(read).epoch().get();
        let ended = refused_once_discarded(&mut client, read, epoch).await?;
        w.kernel.numbers = vec![u64::from(ended.code.number()), read];
        Ok(())
    })
}

#[then("the discard version stays at or below the Bind's version until the transaction ends")]
fn transaction_held(w: &mut Steps) -> TestResult {
    assert!(w.kernel.refused.is_empty(), "{:?}", w.kernel.refused);
    let batch = w.kernel.batches.first().context("the read")?;
    assert_eq!(Some(&batch.version), w.kernel.numbers.get(1));
    assert_eq!(
        w.kernel.numbers.first(),
        Some(&u64::from(ErrorCode::TooOld.number()))
    );
    w.roles = None;
    Ok(())
}
