/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The store over the write-ahead log on its simulated storage, whose futures complete when first polled.

use std::path::PathBuf;

use std::num::NonZeroU32;

use ultragraphdb_common_config::{
    CONFIG_LOG_CLUSTER, ConfigError, ConfigErrorEnum, ConfigRecord, ConfigStore,
};
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

/// Appends `record` to `log` as the store does, durable once this returns.
fn append(log: &mut Wal<SimStorage>, record: &ConfigRecord<'_>) -> TestResult {
    let (tail, parent) = log.tail();
    let position = tail.next().context("a next position")?;
    let body = record.encode();
    let _ticket = log.append(RoundRef::new(
        position,
        parent,
        0,
        1,
        NonZeroU32::MIN,
        &body,
    ))?;
    let _synced = block_on(log.sync())??;
    Ok(())
}

/// Recovery refuses a record that does not follow from the state before it, naming the record's position: a revision
/// skipping a number, the activation of a revision not stored, the retirement of a target that does not exist.
#[test]
fn test_a_record_not_following_its_state_diverges_at_its_position() -> TestResult {
    let files = bundle(7);
    let records = [
        ConfigRecord::Revision {
            target: "ci",
            number: 2,
            activate: false,
            files: &files,
        },
        ConfigRecord::Activate {
            target: "ci",
            number: 1,
        },
        ConfigRecord::Retire { target: "ci" },
    ];
    for (seed, record) in (6..).zip(records) {
        let storage = SimStorage::new(seed);
        let mut log = open_log(&storage)?;
        append(&mut log, &record)?;
        let position = log.committed().get();
        let refused = block_on(ConfigStore::recover(log))?
            .err()
            .context("a refusal")?;
        assert!(
            matches!(refused.0, ConfigErrorEnum::ReplayDiverged { position: at, .. } if at == position),
            "{record:?}: {refused}"
        );
        assert!(
            refused.to_string().contains(&position.to_string()),
            "{refused}"
        );
    }
    Ok(())
}

/// Only a target's active revision is checked: a target holding revisions and none active has no defect, while an active
/// revision whose files no longer read as a target is one defect naming its revision.
#[test]
fn test_only_an_active_revision_has_defects() -> TestResult {
    let storage = SimStorage::new(9);
    let mut store = block_on(ConfigStore::recover(open_log(&storage)?))??;
    assert_eq!(block_on(store.add_revision("ci", 0, &bundle(7)))??, 1);
    assert!(store.active_defects().is_empty());
    drop(store);
    let mut log = open_log(&storage)?;
    append(
        &mut log,
        &ConfigRecord::Revision {
            target: "broken",
            number: 1,
            activate: true,
            files: b"not a bundle",
        },
    )?;
    let recovered = block_on(ConfigStore::recover(log))??;
    let defects = recovered.active_defects();
    let refused = TargetFiles::from_bundle("broken", b"not a bundle")
        .err()
        .context("a refused bundle")?;
    assert_eq!(defects, vec![(*refused).of_revision(1)]);
    Ok(())
}

/// Activating a revision that changes what the active one fixes is refused, and retiring a target that does not exist
/// names it.
#[test]
fn test_an_activation_and_a_retirement_are_refused_for_their_reasons() -> TestResult {
    let mut store = active_store(&SimStorage::new(10))?;
    let mut log = open_log(&SimStorage::new(11))?;
    let first = bundle(7);
    let second = bundle(8);
    for (number, files) in [(1, &first), (2, &second)] {
        append(
            &mut log,
            &ConfigRecord::Revision {
                target: "ci",
                number,
                activate: number == 1,
                files,
            },
        )?;
    }
    let mut two = block_on(ConfigStore::recover(log))??;
    let refused = block_on(two.activate("ci", 2))?
        .err()
        .context("a refusal")?;
    assert!(
        matches!(&refused.0, ConfigErrorEnum::Refused { defects } if defects.iter().any(|d| d.setting() == Some("cluster.id"))),
        "{refused}"
    );
    assert_eq!(
        block_on(store.retire("nobody"))?,
        Err(ConfigError::unknown_target("nobody"))
    );
    Ok(())
}
