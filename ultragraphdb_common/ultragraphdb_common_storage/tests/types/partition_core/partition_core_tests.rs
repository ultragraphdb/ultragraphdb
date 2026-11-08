/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A partition's core as it serves: its turns, its time, what it retains, what it tells at a stop and at a transaction's limit,
//! and what it shares with its peers, against a core of `core_harness`.

use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use futures::channel::mpsc::unbounded;
use ultragraphdb_cluster_partition::PartitionMessage;
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_common_storage::{CLOSES_PER_TURN_MAX, Peers};
use ultragraphdb_core_ids::{Epoch, Version};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{ErrorCode, Scan, ScanAt, ScanRange};
use ultragraphdb_store_quads::{iri_bucket, term_key};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_test::TestDir;

use crate::types::partition_core::core_harness::{
    CoreUnderTest, DUE, PARTITION, PRINCIPAL, Refused, Setup, TENANT, bind, commit, edges_write,
    ex, next_frame, on_core, refusal_of, role_cluster, scanned,
};

/// How often a test looks again for what the core does in its own time.
const POLL: Duration = Duration::from_millis(2);

/// A frontend's address nothing listens at: whatever the core asks of it fails.
fn unreachable() -> TestResult<Address> {
    Ok(Address::parse("tcp://127.0.0.1:1")?)
}

/// The close of `epoch`, delivered to the core as the role's shell delivers one.
fn close(core: &CoreUnderTest, epoch: u64) -> TestResult {
    let close = Close::new(Epoch::new(epoch)?, 0, Vec::new(), Vec::new())?;
    core.closes.send(PartitionMessage::Close(close))?;
    Ok(())
}

/// The partition's visibility watermark, as a Bind answers it.
async fn watermark(client: &mut RoleClient, identity: u8) -> TestResult<u64> {
    Ok(bind(client, identity).await?.1)
}

/// The first discard version the core raises within [`DUE`].
async fn discarded(core: &CoreUnderTest) -> TestResult<u64> {
    for _ in 0..DUE.as_millis() / POLL.as_millis() {
        match core.discards.try_recv() {
            Ok(version) => return Ok(version),
            Err(TryRecvError::Empty) => compio::time::sleep(POLL).await,
            Err(TryRecvError::Disconnected) => return Err(TestError::new("the core is gone")),
        }
    }
    Err(TestError::new("no discard version within its time"))
}

/// The name a core dials its peers under, the frontend it asks and its cluster, as given.
#[test]
fn test_peers_hold_what_a_core_dials_with() -> TestResult {
    let dir = TestDir::new("peers");
    let cluster = role_cluster(dir.path())?;
    let frontend = unreachable()?;
    let (own, _mailbox) = unbounded();
    let peers = Peers::new(
        cluster.clone(),
        frontend.clone(),
        "storage".to_string(),
        own,
    );
    assert_eq!(peers.name(), "storage");
    assert_eq!(peers.frontend(), &frontend);
    assert_eq!(peers.cluster().id(), cluster.id());
    Ok(())
}

/// A core shows the partition it hosts.
#[test]
fn test_a_core_shows_its_partition() -> TestResult {
    on_core(
        "core-debug",
        Setup::default(),
        unreachable()?,
        async |core| {
            let id = format!("{:?}", PartitionId::new(PARTITION)?);
            assert!(core.debug.starts_with("PartitionCore"), "{}", core.debug);
            assert!(core.debug.contains(&id), "{} names {id}", core.debug);
            Ok(())
        },
    )
}

/// A commit is answered on its stream once durable, stamped one above the latest close delivered, none yet: in epoch 1.
#[test]
fn test_a_commit_is_answered_in_the_epoch_after_the_last_close() -> TestResult {
    on_core(
        "core-commit",
        Setup::default(),
        unreachable()?,
        async |core| {
            let mut client = core.dial().await?;
            let version = commit(&mut client, &edges_write(&[("a", "b")])).await?;
            assert_eq!(Version::from_u64(version).epoch(), Epoch::new(1)?);
            Ok(())
        },
    )
}

/// The epoch of a commit made once `count` closes, of epochs 1 on, reached the core at once: one above the last close it took.
fn committed_after_closes(name: &str, count: usize) -> TestResult<Epoch> {
    let mut epoch = None;
    on_core(name, Setup::default(), unreachable()?, async |core| {
        for closed in 1..u64::try_from(count)? + 1 {
            close(&core, closed)?;
        }
        // The commit is stamped two turns or more after the closes were sent: its Bind is answered in one, and its commit taken
        // in a later one.
        let mut client = core.dial().await?;
        let version = commit(&mut client, &edges_write(&[("a", "b")])).await?;
        epoch = Some(Version::from_u64(version).epoch());
        Ok(())
    })?;
    epoch.context("the commit's epoch")
}

/// A turn takes as many closes as its bound: a commit after them is stamped in the epoch above the last.
#[test]
fn test_a_turn_takes_closes_up_to_its_bound() -> TestResult {
    let epoch = committed_after_closes("core-closes-bound", CLOSES_PER_TURN_MAX)?;
    assert_eq!(epoch.get(), u64::try_from(CLOSES_PER_TURN_MAX)? + 1);
    Ok(())
}

/// A close past a turn's bound waits for the next turn, and is taken there, not lost.
#[test]
fn test_a_close_past_a_turns_bound_is_taken_by_the_next() -> TestResult {
    let past = CLOSES_PER_TURN_MAX + 1;
    let epoch = committed_after_closes("core-closes-past", past)?;
    assert_eq!(epoch.get(), u64::try_from(past)? + 1);
    Ok(())
}

/// A Scan waiting for an epoch the partition does not close is refused once its deadline passes, on its stream: code 24, `detail`
/// the partition (`PROTOCOL.md` §13.13). The core's time moves on by itself.
#[test]
fn test_a_scan_past_its_deadline_is_refused_on_its_stream() -> TestResult {
    on_core(
        "core-deadline",
        Setup::default(),
        unreachable()?,
        async |core| {
            let mut client = core.dial().await?;
            let key = term_key(TENANT, &ex("a"));
            let past = [key.as_slice(), &[0]].concat();
            let mut scan = Scan::new(ScanAt::LatestClosed, 0, 5, PRINCIPAL, [0; 16], 0, 50);
            scan.push_range(ScanRange::new(PARTITION, &key, &past));
            let (batches, refused) = scanned(&mut client, &scan).await?;
            assert!(batches.is_empty(), "{batches:?}");
            let refused = refused.context("the refusal")?;
            assert_eq!(
                (refused.code, refused.detail),
                (ErrorCode::TimedOut, u64::from(PARTITION))
            );
            assert!(
                refused.message.contains("has not closed epoch 5"),
                "{refused:?}"
            );
            Ok(())
        },
    )
}

/// An epoch closed longer ago than the retention raises the discard version to its version bound, at most the visibility
/// watermark (`PROTOCOL.md` §11.9).
#[test]
fn test_an_epoch_past_its_retention_raises_the_discard_version() -> TestResult {
    let setup = Setup {
        retain_ms: 1,
        ..Setup::default()
    };
    on_core("core-discard", setup, unreachable()?, async |core| {
        let mut client = core.dial().await?;
        let _version = commit(&mut client, &edges_write(&[("a", "b")])).await?;
        close(&core, 1)?;
        let raised = discarded(&core).await?;
        let watermark = watermark(&mut client, 1).await?;
        assert_eq!(raised, Version::bound_of(Epoch::new(1)?).min(watermark));
        Ok(())
    })
}

/// A partition that applied nothing raises no discard version, however long ago its epoch closed: the bound it may rise to is
/// its watermark, which is where the store's discard version stands.
#[test]
fn test_a_partition_that_applied_nothing_raises_no_discard_version() -> TestResult {
    let setup = Setup {
        retain_ms: 1,
        ..Setup::default()
    };
    on_core("core-no-discard", setup, unreachable()?, async |core| {
        let mut client = core.dial().await?;
        close(&core, 1)?;
        assert_eq!(watermark(&mut client, 1).await?, 0, "nothing applied");
        // Many retentions and many turns.
        compio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(core.discards.try_recv(), Err(TryRecvError::Empty));
        Ok(())
    })
}

/// An IRI of `partition` among `partitions`, the first named `<at>-<n>` that is.
pub(crate) fn owned_by(partition: u32, partitions: u32, at: &str) -> TestResult<String> {
    let map = PlacementMap::even(partitions, NodeId::new(1))?;
    (0..100_000)
        .map(|n| format!("{at}-{n}"))
        .find(|name| {
            iri_bucket(TENANT, &ex(name))
                .is_some_and(|bucket| map.partition_of(bucket, Epoch::ZERO).get() == partition)
        })
        .context("an IRI of the partition")
}

/// A Write of partition 1 naming an entity partition 2 owns, its home in no process the core reaches: its Bind stream, the Write
/// left waiting on its home.
pub(crate) async fn parked_write(client: &mut RoleClient) -> TestResult<u64> {
    let (subject, object) = (owned_by(1, 2, "s")?, owned_by(2, 2, "o")?);
    let (stream, _) = bind(client, 3).await?;
    client
        .send_on(stream, &edges_write(&[(&subject, &object)]))
        .await?;
    Ok(stream)
}

/// A stop answers each request parked on a home with code 17 on its stream, since none reached the commit step (`PROTOCOL.md`
/// §11.12).
#[test]
fn test_a_stop_answers_a_parked_write_as_unavailable() -> TestResult {
    let setup = Setup {
        partitions: 2,
        ..Setup::default()
    };
    on_core("core-stop", setup, unreachable()?, async |core| {
        let mut client = core.dial().await?;
        let stream = parked_write(&mut client).await?;
        let silent = compio::time::timeout(Duration::from_millis(50), client.next_frame()).await;
        assert!(silent.is_err(), "the Write waits on its home");
        core.stopping.set();
        let told = next_frame(&mut client).await?;
        let told: Refused = refusal_of(&told)?.context("an Error")?;
        assert_eq!((told.stream, told.code), (stream, ErrorCode::Unavailable));
        assert!(told.message.contains("the partition stopped"), "{told:?}");
        Ok(())
    })
}

/// A transaction that passes its limit, 10 s from its Bind, is told so on its Bind stream: code 4, `detail` the limit in
/// milliseconds (`PROTOCOL.md` §10).
#[test]
fn test_a_transaction_past_its_limit_is_told_so() -> TestResult {
    const LIMIT_MS: u64 = 10_000;
    on_core(
        "core-limit",
        Setup::default(),
        unreachable()?,
        async |core| {
            let mut client = core.dial().await?;
            let (stream, _) = bind(&mut client, 4).await?;
            let wait = Duration::from_millis(LIMIT_MS) + DUE;
            let told = compio::time::timeout(wait, client.next_frame())
                .await
                .context("the limit's Error")??;
            let told = refusal_of(&told)?.context("an Error")?;
            assert_eq!(
                (told.stream, told.code, told.detail),
                (stream, ErrorCode::PastLimit, LIMIT_MS)
            );
            Ok(())
        },
    )
}

/// A partition whose open epoch is full asks the frontend to close it once a commit needs a version past it (commit-rounds,
/// "A full epoch asks for a close and waits").
#[test]
fn test_a_full_epoch_asks_the_frontend_for_its_close() -> TestResult {
    use crate::types::partition_core::core_rig::{RigSetup, TENANT as RIG_TENANT, with_core};
    use crate::types::partition_core::core_writes::edges;
    use futures::StreamExt;
    use ultragraphdb_common_storage::FrontendAsk;
    let setup = RigSetup {
        full: true,
        ..RigSetup::default()
    };
    with_core("core-full-epoch", setup, async |mut core| {
        let mut client = core.dial().await?;
        client
            .send_only(&edges(RIG_TENANT, &[("a", "p", "b")]))
            .await?;
        let open = Epoch::ZERO.next()?;
        let asked = compio::time::timeout(DUE, async {
            while let Some(ask) = core.asks.next().await {
                if let FrontendAsk::Close(epoch) = ask {
                    return Some(epoch);
                }
            }
            None
        })
        .await
        .context("a close asked for within its time")?;
        assert_eq!(asked, Some(open));
        Ok(())
    })
}
