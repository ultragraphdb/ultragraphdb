/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The store over the write-ahead log on its simulated storage, whose futures complete when first polled.

use std::path::PathBuf;

use ultragraphdb_common_config::{CONFIG_LOG_CLUSTER, ConfigError, ConfigErrorEnum, ConfigStore};
use ultragraphdb_common_config_check::TargetFiles;
use ultragraphdb_error_log::LogError;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_log::{DurabilityTicket, LogEvent, OrderedLog, Position, Round, RoundRef};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};

const FRONTEND: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\npartition_count = 1\n[observability]\naddress = \"tcp://127.0.0.1:18081\"\n[frontend]\nrole_address = \"tcp://10.0.0.1:7081\"\ndata_dir = \"/data\"\n[sequencer]\nepoch_ms_min = 2\nepoch_ms_max = 10\n";
const STORAGE: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nfrontend_address = \"tcp://frontend:7081\"\n[observability]\naddress = \"tcp://0.0.0.0:8080\"\n[storage]\npartition_count = 1\ndata_dir = \"/data\"\nrole_address = \"tcp://10.0.0.2:7000\"\n";

/// Runs a future of the simulated storage, which completes when first polled.
fn block_on<F: Future>(future: F) -> TestResult<F::Output> {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    for _ in 0..1_000 {
        if let std::task::Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return Ok(output);
        }
    }
    Err(TestError::new(
        "a simulated future still pending after 1,000 polls",
    ))
}

fn open_log(storage: &SimStorage) -> TestResult<Wal<SimStorage>> {
    let config = WalConfig::new(CONFIG_LOG_CLUSTER, false);
    let (log, _) = block_on(Wal::open(storage.clone(), PathBuf::from("/wal"), config))??;
    Ok(log)
}

/// Target `ci`'s files, every file's `id = 7` replaced by `id = {id}`.
fn bundle(id: u64) -> Vec<u8> {
    let named = |text: &str| text.replace("id = 7", &format!("id = {id}")).into_bytes();
    let mut target = TargetFiles::new("ci");
    target.insert("frontend", "frontend.toml", named(FRONTEND));
    target.insert("storage", "storage.toml", named(STORAGE));
    target.to_bundle()
}

/// A store over `storage` holding target `ci`'s revision 1, active.
fn active_store(storage: &SimStorage) -> TestResult<ConfigStore<Wal<SimStorage>>> {
    let mut store = block_on(ConfigStore::recover(open_log(storage)?))??;
    assert_eq!(block_on(store.add_revision("ci", 0, &bundle(7)))??, 1);
    block_on(store.activate("ci", 1))??;
    Ok(store)
}

/// Each target is listed by its name, retired once retired.
#[test]
fn test_each_target_is_listed_by_name() -> TestResult {
    let mut store = active_store(&SimStorage::new(3))?;
    let listed = store.targets();
    let ci = listed.first().context("target ci")?;
    assert_eq!(
        (ci.name(), ci.latest(), ci.active(), ci.retired()),
        ("ci", 1, Some(1), false)
    );
    block_on(store.retire("ci"))??;
    assert!(store.targets().first().context("target ci")?.retired());
    Ok(())
}

/// A revision naming another cluster than the active one is refused when added and when activated: the active revision
/// fixes the cluster's identity.
#[test]
fn test_the_active_revision_fixes_what_follows() -> TestResult {
    let storage = SimStorage::new(4);
    let mut store = active_store(&storage)?;
    let refused = block_on(store.add_revision("ci", 1, &bundle(8)))?
        .err()
        .context("a refusal")?;
    assert!(
        matches!(&refused.0, ConfigErrorEnum::Refused { defects } if defects.iter().any(|d| d.setting() == Some("cluster.id"))),
        "{refused}"
    );
    Ok(())
}

/// A log whose reads return nothing while it holds durable rounds: a log breaking its own contract.
struct EmptyReads(Wal<SimStorage>);

impl OrderedLog for EmptyReads {
    fn append(&mut self, round: RoundRef<'_>) -> Result<DurabilityTicket, LogError> {
        self.0.append(round)
    }

    fn poll_ticket(&self, ticket: DurabilityTicket) -> std::task::Poll<Result<(), LogError>> {
        self.0.poll_ticket(ticket)
    }

    fn sync(&mut self) -> impl Future<Output = Result<Position, LogError>> {
        self.0.sync()
    }

    fn committed(&self) -> Position {
        self.0.committed()
    }

    fn tail(&self) -> (Position, u64) {
        self.0.tail()
    }

    async fn read(&mut self, _from: Position, _max_bytes: u32) -> Result<Vec<Round>, LogError> {
        Ok(Vec::new())
    }

    fn trim_through(&mut self, position: Position) -> impl Future<Output = Result<(), LogError>> {
        self.0.trim_through(position)
    }

    fn checkpoint(&self) -> Position {
        self.0.checkpoint()
    }

    fn seal(&mut self) -> Result<Position, LogError> {
        self.0.seal()
    }

    fn next_event(&mut self) -> Option<LogEvent> {
        self.0.next_event()
    }
}

/// Recovery that reads nothing where a durable round stands names that round's position, the first.
#[test]
fn test_a_log_reading_nothing_diverges_at_its_first_round() -> TestResult {
    let storage = SimStorage::new(5);
    drop(active_store(&storage)?);
    let refused = block_on(ConfigStore::recover(EmptyReads(open_log(&storage)?)))?
        .err()
        .context("a refusal")?;
    assert_eq!(refused, ConfigError::replay_diverged(1, "a durable round"));
    Ok(())
}
