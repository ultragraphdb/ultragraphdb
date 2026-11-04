/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::mpsc::Sender;
use std::thread;

use ultragraphdb_cluster_partition::{
    Fanout, PartitionHost, PartitionMailbox, PartitionMessage, ReverseQuery,
};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_core_keys::Graph;
use ultragraphdb_core_mapping::Change;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, RoundBounds};
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};

use crate::utils::cluster::{Cluster, NS, Part, changeset, epoch, ex, insert, node, pid};

/// How many turns a partition's thread takes at most: far past what the exchange needs.
const TURNS_MAX: u32 = 200_000;

/// Partition `part`'s thread body: a compio runtime of its own, a log in `dir`, and a host taking `changes` in turn
/// until each has its outcome, and then serving until `peers_done` says every partition's write has ended.
fn serve(
    part: Part,
    mailbox: PartitionMailbox,
    dir: &std::path::Path,
    changes: Vec<Change>,
    done: &Sender<()>,
    all_done: &std::sync::atomic::AtomicBool,
) -> TestResult<Part> {
    let runtime = compio::runtime::Runtime::new().context("a runtime")?;
    runtime.block_on(async {
        let (log, _) = Wal::open(CompioStorage, dir, WalConfig::new(1, true))
            .await
            .map_err(|e| TestError::new(e.to_string()))?;
        let pipeline = Pipeline::new(part.applier(), log, RoundBounds::new(u32::MAX, u32::MAX))?
            .with_partition(part.id().get(), epoch(1)?, CloseRequest::default(), &[])
            .bound(part.id(), part.placement());
        let mut host = PartitionHost::new(&part, pipeline, mailbox);
        let mut waiting = changes.len();
        for change in changes {
            let _ticket = host.submit(changeset(vec![change]), 0)?;
        }
        if waiting == 0 {
            done.send(())?;
        }
        for _ in 0..TURNS_MAX {
            host.turn(0).await?;
            for outcome in host.take_outcomes() {
                if let Err(refusal) = outcome.outcome() {
                    return Err(TestError::new(refusal.to_string()));
                }
                waiting -= 1;
                if waiting == 0 {
                    done.send(())?;
                }
            }
            if all_done.load(std::sync::atomic::Ordering::Acquire) {
                drop(host);
                return Ok(());
            }
            compio::time::sleep(std::time::Duration::from_micros(50)).await;
        }
        Err(TestError::new("the partition never finished"))
    })?;
    Ok(part)
}

/// Three partitions, each on its own thread and compio runtime with a real log, exchange registrations as messages:
/// edges from partitions 1 and 3 to an entity homed in partition 2 commit, and a reverse read at a closed epoch gathers
/// both from the partitions its referrer set names.
#[expect(
    clippy::disallowed_methods,
    reason = "each partition runs on a thread and a runtime of its own, as the storage process runs them"
)]
#[test]
fn test_partitions_on_their_own_runtimes_exchange_registrations() -> TestResult {
    let cluster = Cluster::even(3)?;
    let a = cluster.name_in(1, "a")?;
    let b = cluster.name_in(2, "b")?;
    let c = cluster.name_in(3, "c")?;
    let knows = cluster.name_in(1, "knows")?;
    let placement = PlacementMap::even(3, NodeId::new(1))?;
    let ids = (1..=3)
        .map(PartitionId::new)
        .collect::<Result<Vec<_>, _>>()?;
    let root = std::env::temp_dir().join(format!("ug_hosts_{}", std::process::id()));
    let mailboxes = PartitionMailbox::mesh(&ids);
    let closers: Vec<Sender<PartitionMessage>> = mailboxes
        .iter()
        .map(|m| m.sender(m.id()).context("a sender to itself"))
        .collect::<TestResult<_>>()?;
    let (done, finished) = std::sync::mpsc::channel();
    let all_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut writes = vec![
        vec![insert(&a, &knows, &b)],
        Vec::new(),
        vec![insert(&c, &knows, &b)],
    ]
    .into_iter();
    let mut threads = Vec::new();
    for (mailbox, id) in mailboxes.into_iter().zip(&ids) {
        let part = node(*id, &placement, epoch(1)?)?;
        let dir = root.join(id.get().to_string());
        std::fs::create_dir_all(&dir)?;
        let changes = writes.next().context("a partition's writes")?;
        let (done, all_done) = (done.clone(), all_done.clone());
        threads.push(thread::spawn(move || {
            serve(part, mailbox, &dir, changes, &done, &all_done).map_err(|e| e.to_string())
        }));
    }
    for _ in &ids {
        finished
            .recv_timeout(std::time::Duration::from_secs(30))
            .context("every partition's writes end")?;
    }
    // The epoch's close reaches every partition, and each serves it before stopping.
    let close = cluster.closed().next()?;
    for closer in &closers {
        closer.send(PartitionMessage::Close(Close::new(
            close,
            0,
            Vec::new(),
            Vec::new(),
        )?))?;
    }
    thread::sleep(std::time::Duration::from_millis(50));
    all_done.store(true, std::sync::atomic::Ordering::Release);
    let parts = threads
        .into_iter()
        .map(|t| {
            t.join()
                .map_err(|_| TestError::new("a partition's thread panicked"))?
                .map_err(TestError::new)
        })
        .collect::<TestResult<Vec<Part>>>()?;
    std::fs::remove_dir_all(&root)?;
    let object = id_in(&parts, 2, &b)?;
    let predicate = id_in(&parts, 1, &knows)?;
    let query = ReverseQuery::new(pid(2)?, NS, Graph::Default, predicate, object, close);
    let mut subjects = Vec::new();
    let asked = Fanout::new(&parts).subjects(&query, &mut subjects)?;
    assert_eq!(asked, vec![pid(1)?, pid(3)?]);
    let mut expected = vec![id_in(&parts, 1, &a)?, id_in(&parts, 3, &c)?];
    expected.sort();
    assert_eq!(subjects, expected);
    Ok(())
}

/// The ID partition `id` of `parts` holds for `<name>`.
fn id_in(parts: &[Part], id: u32, name: &str) -> TestResult<ultragraphdb_core_ids::SystemId> {
    parts
        .get(id as usize - 1)
        .context("the partition")?
        .applier()
        .quads()
        .id_of(NS, ex(name).as_str())?
        .context("the partition holds the ID")
}
