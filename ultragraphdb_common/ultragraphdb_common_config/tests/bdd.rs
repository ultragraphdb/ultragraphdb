/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The revisions' scenarios, bound to `features/`: the store over the write-ahead log on its simulated storage, whose power
//! loss drops everything not synced.

use std::path::PathBuf;

use cucumber::{World, then, when};
use ultragraphdb_common_config::{CONFIG_LOG_CLUSTER, ConfigErrorEnum, ConfigRecord, ConfigStore};
use ultragraphdb_common_config_check::TargetFiles;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_log::{OrderedLog, RoundRef};
use ultragraphdb_transactions_log::{FaultMode, SimStorage, Wal, WalConfig};

type Store = ConfigStore<Wal<SimStorage>>;

const FRONTEND: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\npartition_count = 1\n[observability]\naddress = \"tcp://127.0.0.1:18081\"\n[frontend]\nrole_address = \"tcp://10.0.0.1:7081\"\ndata_dir = \"/data\"\n[sequencer]\nepoch_ms_min = 2\nepoch_ms_max = 10\n";
const STORAGE: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nfrontend_address = \"tcp://frontend:7081\"\n[observability]\naddress = \"tcp://0.0.0.0:8080\"\n[storage]\npartition_count = 1\ndata_dir = \"/data\"\nrole_address = \"tcp://10.0.0.2:7000\"\n";

#[derive(World)]
struct Steps {
    storage: SimStorage,
    store: Option<Store>,
    refused: Option<ConfigErrorEnum>,
    differing: Vec<(String, Option<u64>)>,
}

impl std::fmt::Debug for Steps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Steps")
    }
}

impl Default for Steps {
    fn default() -> Self {
        Self {
            storage: SimStorage::new(7),
            store: None,
            refused: None,
            differing: Vec::new(),
        }
    }
}

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

fn open_store(storage: &SimStorage) -> TestResult<Store> {
    Ok(block_on(ConfigStore::recover(open_log(storage)?))??)
}

/// A valid set of files; `storage` adds `extra` to the storage file.
fn files(extra: &str) -> TargetFiles {
    let mut target = TargetFiles::new("ci");
    target.insert("frontend", "frontend.toml", FRONTEND.as_bytes().to_vec());
    target.insert(
        "storage",
        "storage.toml",
        format!("{STORAGE}{extra}").into_bytes(),
    );
    target
}

/// A store holding `count` revisions of target `ci`, each differing in a comment, none active.
fn with_revisions(w: &mut Steps, count: u64) -> TestResult<&mut Store> {
    let mut store = open_store(&w.storage)?;
    for number in 1..=count {
        let body = files(&format!("# revision {number}\n")).to_bundle();
        let _number = block_on(store.add_revision("ci", number - 1, &body))??;
    }
    Ok(w.store.insert(store))
}

fn store(w: &mut Steps) -> TestResult<&mut Store> {
    w.store.as_mut().context("a store")
}

#[when("target `ci` holds revisions 1 and 2, with 2 active, and an administrator activates 1")]
fn rollback(w: &mut Steps) -> TestResult {
    let store = with_revisions(w, 2)?;
    block_on(store.activate("ci", 2))??;
    block_on(store.activate("ci", 1))??;
    Ok(())
}

#[then("a role starting next receives revision 1, and revision 2 is still stored")]
fn rolled_back(w: &mut Steps) -> TestResult {
    let store = store(w)?;
    let (number, rendered) = store.instance("ci", None, "storage")?;
    assert_eq!(number, 1);
    assert!(rendered.contains("partition_count = 1"), "{rendered}");
    assert_eq!(store.instance("ci", Some(2), "storage")?.0, 2);
    assert!(!store.files("ci", 2)?.is_empty());
    assert_eq!(store.targets().first().context("a target")?.latest(), 2);
    Ok(())
}

#[when("two administrators edit from revision 3, and the first adds revision 4")]
fn stale(w: &mut Steps) -> TestResult {
    let store = with_revisions(w, 3)?;
    let first = files("# the first\n").to_bundle();
    assert_eq!(block_on(store.add_revision("ci", 3, &first))??, 4);
    let second = files("# the second\n").to_bundle();
    let refused = block_on(store.add_revision("ci", 3, &second))?
        .err()
        .context("a refusal")?;
    w.refused = Some(refused.0);
    Ok(())
}

#[then("the second's revision is refused naming revision 4, and nothing is stored")]
fn refused_stale(w: &mut Steps) -> TestResult {
    let refused = w.refused.clone().context("a refusal")?;
    assert!(
        matches!(refused, ConfigErrorEnum::Stale { latest: 4, .. }),
        "{refused:?}"
    );
    assert_eq!(store(w)?.targets().first().context("a target")?.latest(), 4);
    Ok(())
}

#[when("a revision is acknowledged and the service is killed and restarted")]
fn killed(w: &mut Steps) -> TestResult {
    let store = with_revisions(w, 2)?;
    block_on(store.activate("ci", 2))??;
    w.storage.power_loss();
    w.store = Some(open_store(&w.storage)?);
    Ok(())
}

#[then("the revision is stored, and the active revision is the one last acknowledged")]
fn survived(w: &mut Steps) -> TestResult {
    let store = store(w)?;
    let target = store.targets();
    let target = target.first().context("a target")?;
    assert_eq!((target.latest(), target.active()), (2, Some(2)));
    assert_eq!(store.files("ci", 2)?, files("# revision 2\n").to_bundle());
    Ok(())
}

#[when("the service is killed while appending a revision it has not acknowledged")]
fn torn(w: &mut Steps) -> TestResult {
    let _store = with_revisions(w, 1)?;
    let mut log = open_log(&w.storage)?;
    let body = ConfigRecord::Revision {
        target: "ci",
        number: 2,
        activate: false,
        files: &files("# unacknowledged\n").to_bundle(),
    }
    .encode();
    let (tail, parent) = log.tail();
    let position = tail.next().context("a position")?;
    let _ticket = log.append(RoundRef::new(
        position,
        parent,
        0,
        1,
        std::num::NonZeroU32::MIN,
        &body,
    ))?;
    // The write tears partway through a block, as a kill during it would.
    w.storage
        .fail_at(w.storage.trace().len(), FaultMode::ShortTornWrite);
    let _failed = block_on(log.sync())?;
    w.storage.power_loss();
    w.store = Some(open_store(&w.storage)?);
    Ok(())
}

#[then("after a restart the revision is absent or whole, never partial")]
fn absent_or_whole(w: &mut Steps) -> TestResult {
    let store = store(w)?;
    let latest = store.targets().first().context("a target")?.latest();
    assert_eq!(latest, 1, "the revision was never synced");
    assert_eq!(store.files("ci", 1)?, files("# revision 1\n").to_bundle());
    Ok(())
}

#[when("a service starts declaring target `ci`, and again after a declared file changed validly")]
fn seeded_twice(w: &mut Steps) -> TestResult {
    let mut first = open_store(&w.storage)?;
    assert!(block_on(first.seed(&[files("")]))??.is_empty());
    w.storage.power_loss();
    let mut second = open_store(&w.storage)?;
    w.differing = block_on(second.seed(&[files("# changed\n")]))??;
    w.store = Some(second);
    Ok(())
}

#[then(
    "`ci` holds revision 1 alone, and the second start reports that the file differs and the revision still active"
)]
fn seeded_once(w: &mut Steps) -> TestResult {
    assert_eq!(w.differing, [("ci".to_string(), Some(1))]);
    let store = store(w)?;
    let targets = store.targets();
    let target = targets.first().context("a target")?;
    assert_eq!((target.latest(), target.active()), (1, Some(1)));
    assert_eq!(store.files("ci", 1)?, files("").to_bundle());
    Ok(())
}

#[when("a store's active revision names a setting the release does not hold")]
fn no_longer_accepted(w: &mut Steps) -> TestResult {
    let mut log = open_log(&w.storage)?;
    let body = ConfigRecord::Revision {
        target: "ci",
        number: 1,
        activate: true,
        files: &files("[storage_v9]\nnew_setting = 1\n").to_bundle(),
    }
    .encode();
    let (tail, parent) = log.tail();
    let position = tail.next().context("a position")?;
    let _ticket = log.append(RoundRef::new(
        position,
        parent,
        0,
        1,
        std::num::NonZeroU32::MIN,
        &body,
    ))?;
    let _durable = block_on(log.sync())??;
    w.store = Some(block_on(ConfigStore::recover(log))??);
    Ok(())
}

#[then("the start's check names the target, the revision and the setting")]
fn start_refuses(w: &mut Steps) -> TestResult {
    let defects = store(w)?.active_defects();
    let text = defects
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    for word in ["target ci", "revision 1", "storage_v9"] {
        assert!(text.contains(word), "{word} in: {text}");
    }
    Ok(())
}

#[when("a revision's record holds the most a request can")]
fn maximal(w: &mut Steps) -> TestResult {
    let mut log = open_log(&w.storage)?;
    let files = vec![b'#'; 512 * 1024];
    let body = ConfigRecord::Revision {
        target: "ci",
        number: 1,
        activate: false,
        files: &files,
    }
    .encode();
    let (tail, parent) = log.tail();
    let position = tail.next().context("a position")?;
    let _ticket = log.append(RoundRef::new(
        position,
        parent,
        0,
        1,
        std::num::NonZeroU32::MIN,
        &body,
    ))?;
    let _durable = block_on(log.sync())??;
    w.store = Some(block_on(ConfigStore::recover(log))??);
    Ok(())
}

#[then("the log takes it in one round")]
fn fits(w: &mut Steps) -> TestResult {
    assert_eq!(store(w)?.files("ci", 1)?.len(), 512 * 1024);
    Ok(())
}

#[when("a target is retired")]
fn retired(w: &mut Steps) -> TestResult {
    let store = with_revisions(w, 1)?;
    block_on(store.activate("ci", 1))??;
    block_on(store.retire("ci"))??;
    Ok(())
}

#[then(
    "asking for an instance says it is retired, its revisions are still stored, and activating one serves it again"
)]
fn retirement_ends(w: &mut Steps) -> TestResult {
    let store = store(w)?;
    let refused = store
        .instance("ci", None, "storage")
        .err()
        .context("a refusal")?;
    assert!(refused.to_string().contains("retired"), "{refused}");
    assert!(!store.files("ci", 1)?.is_empty());
    block_on(store.activate("ci", 1))??;
    assert_eq!(store.instance("ci", None, "storage")?.0, 1);
    Ok(())
}

fn main() -> TestResult {
    block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    )
}
