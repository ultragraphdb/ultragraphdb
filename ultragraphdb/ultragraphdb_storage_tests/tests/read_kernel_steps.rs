/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The read kernel through real frames: a real storage process beside a real frontend, a Scan's answer decoded with
//! `EntriesSequence`, `EntriesView` and `ObjectRecords`, so a test reads as compute will.

use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{then, when};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::{PARTITION_INITIAL_CREDIT, RoleClient};
use ultragraphdb_core_ids::{BucketGroup, Epoch, SystemId, Version};
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::Names;
use ultragraphdb_core_terms::{EncodedTerm, Hash128};
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    Abort, Bind, BindKind, BindView, BodyView, Closed, ClosedView, Command, Commit, CommitView,
    Credit, DETAIL_TRANSACTION_NOT_HELD, EntriesEnd, EntriesSequence, ErrorCode, ErrorReplyView,
    FLAG_LAST, MAX_FRAME_FLOOR, MAX_FRAME_SIZE, ObjectRecords, Recall, Reference, ReferenceView,
    RequestTerm, Scan, ScanAt, ScanRange, TermTable, Write, WriteChange,
};
use ultragraphdb_store_quads::{ReferrerSet, iri_bucket, literal_key, tenant_base, term_key};
use ultragraphdb_tests::{CLUSTER, SECRET, STORAGE_ROLE_ADDRESS, scratch, with};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::played_home::{HomeScript, play_home, played_id};
use crate::{
    PREDICATE, Steps, TENANT, commit_at, edge_write, one_process_of_two, subject_of, two_processes,
};

pub(crate) const PARTITION: u32 = 1;
pub(crate) const PRINCIPAL: u64 = 1;
pub(crate) const NO_TRANSACTION: [u8; 16] = [0; 16];

/// What a reader of a Scan took from one frame.
#[derive(Debug, Clone, Default)]
pub struct Batch {
    pub(crate) stream: u64,
    pub(crate) range: u32,
    pub(crate) through: u32,
    pub(crate) version: u64,
    pub(crate) end: u8,
    pub(crate) continues: bool,
    pub(crate) epoch: u64,
    pub(crate) entries: Vec<(Vec<u8>, Vec<u8>)>,
    pub(crate) last: bool,
    pub(crate) size: usize,
}

/// An Error frame.
#[derive(Debug, Clone)]
pub struct Refused {
    pub(crate) stream: u64,
    pub(crate) code: ErrorCode,
    pub(crate) detail: u64,
    pub(crate) message: String,
}

/// What a scenario's When step took for its Then step.
#[derive(Debug, Default)]
pub struct KernelSteps {
    pub(crate) batches: Vec<Batch>,
    pub(crate) refused: Vec<Refused>,
    pub(crate) numbers: Vec<u64>,
    pub(crate) bytes: Vec<Vec<u8>>,
}

/// One frame of a connection, decoded.
pub(crate) enum Frame {
    Entries(Batch),
    Refused(Refused),
    Other(Command),
}

pub(crate) fn address() -> String {
    format!("tcp://{STORAGE_ROLE_ADDRESS}")
}

/// A frontend and one partition's storage; a Scan keeping `stall` milliseconds with no frame able to leave, when given.
pub(crate) fn one_partition(w: &mut Steps, stall: Option<&str>) -> TestResult {
    let settings: Vec<(&str, &str)> = stall
        .map(|ms| ("storage.scan_stall_ms", ms))
        .into_iter()
        .collect();
    partition_holding(w, &settings)
}

/// A frontend and one partition's storage, holding `settings` beside its own.
pub(crate) fn partition_holding(w: &mut Steps, settings: &[(&str, &str)]) -> TestResult {
    use ultragraphdb_tests::{
        FRONTEND_BINARY, FRONTEND_OBSERVABILITY, answering, frontend_holding, role, storage_holding,
    };
    let util = crate::services()?;
    let wait = answering(FRONTEND_OBSERVABILITY, "/health");
    let _frontend: u32 = util.start_service_from_config(role(
        FRONTEND_BINARY,
        &frontend_holding()?,
        &scratch(FRONTEND_BINARY),
        wait,
    ))?;
    let held = settings
        .iter()
        .fold(storage_holding()?, |held, (setting, value)| {
            with(held, setting, value)
        });
    let _storage = crate::start_storage(&util, &held, &scratch("storage-kernel"))?;
    w.roles = Some(util);
    Ok(())
}

pub(crate) async fn dial_at(target: &str) -> TestResult<RoleClient> {
    let target = Address::parse(target)?;
    Ok(RoleClient::dial(&target, CLUSTER, "compute", SECRET).await?)
}

pub(crate) async fn dial() -> TestResult<RoleClient> {
    dial_at(&address()).await
}

/// The next frame of `client`, an Entries frame checked by `sequence` as a dialling side checks it.
pub(crate) async fn next_frame(
    client: &mut RoleClient,
    sequence: &mut EntriesSequence,
) -> TestResult<Frame> {
    decoded(&client.next_frame().await?, sequence)
}

/// `frame` decoded, an Entries frame checked by `sequence`.
pub(crate) fn decoded(frame: &ReceivedFrame, sequence: &mut EntriesSequence) -> TestResult<Frame> {
    let view = frame.view()?;
    let command = view.command();
    Ok(if command == Command::Error {
        let error = ErrorReplyView::from_frame(&view)?;
        Frame::Refused(Refused {
            stream: view.header().stream(),
            code: error.code(),
            detail: error.detail(),
            message: error.message().to_string(),
        })
    } else if command == Command::Entries {
        let entries = sequence.accept(&view)?;
        Frame::Entries(Batch {
            stream: view.header().stream(),
            range: entries.range(),
            through: entries.through(),
            version: entries.version(),
            end: entries.end().number(),
            continues: entries.continues(),
            epoch: entries.epoch(),
            entries: entries
                .entries()
                .iter()
                .map(|entry| {
                    (
                        entry.key().to_vec(),
                        entry.value().unwrap_or_default().to_vec(),
                    )
                })
                .collect(),
            last: view.header().flags() & FLAG_LAST != 0,
            size: frame.bytes().len(),
        })
    } else {
        Frame::Other(command)
    })
}

/// A plain Scan of `ranges`, each a first key and a past key.
pub(crate) fn plain(
    at: ScanAt,
    version: u64,
    epoch: u64,
    deadline_ms: u32,
    ranges: &[(Vec<u8>, Vec<u8>)],
) -> Scan {
    let mut scan = Scan::new(
        at,
        version,
        epoch,
        PRINCIPAL,
        NO_TRANSACTION,
        0,
        deadline_ms,
    );
    for (first, past) in ranges {
        scan.push_range(ScanRange::new(PARTITION, first, past));
    }
    scan
}

/// The range holding `key` alone.
pub(crate) fn alone(key: Vec<u8>) -> (Vec<u8>, Vec<u8>) {
    let past = [key.as_slice(), &[0]].concat();
    (key, past)
}

/// Sends `scan` and reads its answer, granting a frame's credit per frame when `granting`: its frames, and
/// the refusal ending it, if one does.
pub(crate) async fn answer(
    client: &mut RoleClient,
    scan: &Scan,
    granting: bool,
) -> TestResult<(Vec<Batch>, Option<Refused>)> {
    client.send_only(scan).await?;
    let stream = client.last_stream();
    let mut sequence = EntriesSequence::new(scan.ranges());
    let mut batches = Vec::new();
    loop {
        match next_frame(client, &mut sequence).await? {
            Frame::Entries(batch) => {
                let last = batch.last;
                batches.push(batch);
                if last {
                    return Ok((batches, None));
                }
                if granting {
                    client.send_on(stream, &Credit::new(1)).await?;
                }
            }
            Frame::Refused(refused) => return Ok((batches, Some(refused))),
            Frame::Other(command) => {
                return Err(TestError::new(format!(
                    "a frame of command {}",
                    command.number()
                )));
            }
        }
    }
}

/// The ID the partition at `client` holds for `iri`.
pub(crate) async fn id_of(client: &mut RoleClient, iri: &str) -> TestResult<SystemId> {
    let scan = plain(ScanAt::Latest, 0, 0, 0, &[alone(term_key(TENANT, iri))]);
    let (batches, _) = answer(client, &scan, false).await?;
    let (_, value) = batches
        .first()
        .and_then(|batch| batch.entries.first())
        .context("the dictionary's entry")?;
    Ok(SystemId::decode(value)?)
}

/// The partition's latest closed epoch.
pub(crate) async fn closed_at(client: &mut RoleClient) -> TestResult<u64> {
    let reply = client.request(&Closed::request(&[PARTITION])).await?;
    let view = reply.view()?;
    let closed = ClosedView::from_frame(&view)?;
    Ok(closed.partition(0).context("the partition")?.epoch())
}

/// Waits until the partition has closed `epoch`.
pub(crate) async fn closed_by(client: &mut RoleClient, epoch: u64) -> TestResult {
    for _ in 0..5_000 {
        if closed_at(client).await? >= epoch {
            return Ok(());
        }
        compio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    Err(TestError::new(format!("epoch {epoch} did not close")))
}

/// The partition's visibility watermark, as a Bind answers it.
async fn watermark() -> TestResult<u64> {
    let mut client = dial().await?;
    let reply = client.request(&bind_of(0xEE, PRINCIPAL)).await?;
    let view = reply.view()?;
    Ok(BindView::from_frame(&view)?.read_version())
}

pub(crate) fn bind_of(identity: u8, principal: u64) -> Bind {
    Bind::new(
        PARTITION,
        BindKind::Transaction,
        TENANT,
        0,
        principal,
        [identity; 16],
    )
}

/// The key of `subject`'s forward list of the scenarios' predicate, both by the IDs `client`'s partition holds.
pub(crate) async fn forward_key(client: &mut RoleClient, subject: &str) -> TestResult<Vec<u8>> {
    Ok(QuadKey::Forward {
        tenant: TENANT,
        graph: Graph::Default,
        predicate: id_of(client, PREDICATE).await?,
        subject: id_of(client, subject).await?,
    }
    .to_bytes())
}

/// The objects `value`, a list's object records, holds.
pub(crate) fn objects_of(value: &[u8]) -> TestResult<Vec<Vec<u8>>> {
    Ok(ObjectRecords::read(value)?
        .iter()
        .map(|record| record.encoding().to_vec())
        .collect())
}

pub(crate) fn object_of(id: SystemId) -> Vec<u8> {
    EncodedTerm::id(id).as_bytes().to_vec()
}

pub(crate) fn run<T>(future: impl Future<Output = TestResult<T>>) -> TestResult<T> {
    Runtime::new()?.block_on(future)
}

#[when("compute scans the dictionary entry of `<b>` and `R(b)` at `b`'s home")]
fn dictionary_and_referrers(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let b = "http://ex.org/b";
    let group = BucketGroup::of(iri_bucket(TENANT, b).context("a bucket")?);
    let outcome = run(async {
        let mut client = dial().await?;
        let mut asked = Reference::request().in_tenant(TENANT);
        let _at = asked.register_iri(PARTITION, Some(group), b, 0);
        let reply = client.request(&asked).await?;
        let view = reply.view()?;
        let held = ReferenceView::from_frame(&view)?;
        let entity = held
            .registration(0)
            .context("the reply")?
            .entity()
            .context("its ID")?;
        drop(reply);
        let set = QuadKey::Referrers {
            tenant: TENANT,
            entity,
        }
        .to_bytes();
        let ranges = [alone(term_key(TENANT, b)), alone(set.clone())];
        let now = plain(ScanAt::Latest, 0, 0, 0, &ranges);
        let (latest, _) = answer(&mut client, &now, true).await?;
        let before = Version::bound_of(Epoch::ZERO);
        let earlier = plain(ScanAt::Named, before, 0, 0, &ranges);
        let (older, _) = answer(&mut client, &earlier, true).await?;
        Ok::<_, TestError>((entity, group, set, latest, older))
    })?;
    let (entity, member, set, latest, older) = outcome;
    w.kernel.bytes = vec![entity.to_be_bytes().to_vec(), set];
    w.kernel.numbers = vec![u64::from(member.get())];
    w.kernel.batches = latest;
    w.kernel.batches.extend(older);
    Ok(())
}

#[then("the answer holds `b`'s ID and its referrer set's members at that version")]
fn holds_id_and_members(w: &mut Steps) -> TestResult {
    let [latest_id, latest_set, older_id, older_set] = w.kernel.batches.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.kernel.batches)));
    };
    let id = w.kernel.bytes.first().context("the home's ID")?;
    let set_key = w.kernel.bytes.get(1).context("the set's key")?;
    let (_, held) = latest_id.entries.first().context("the entry")?;
    assert_eq!(held, id, "the dictionary holds what the home answered");
    let (key, value) = latest_set.entries.first().context("the set")?;
    assert_eq!(key, set_key);
    let set = ReferrerSet::decode(key, value)?;
    let group = *w.kernel.numbers.first().context("the group")?;
    let groups: Vec<u64> = set
        .members()
        .iter()
        .map(|m| u64::from(m.group().get()))
        .collect();
    assert_eq!(groups, [group]);
    assert_eq!(
        older_id.entries.len(),
        1,
        "the dictionary holds no versions"
    );
    assert!(
        older_set.entries.is_empty(),
        "a referrer set is versioned: {older_set:?}"
    );
    assert_eq!(older_set.end, EntriesEnd::Ended.number());
    w.roles = None;
    Ok(())
}

/// How many subjects fit a write set of one transaction, well within its limit.
const SUBJECTS_PER_COMMIT: u64 = 2_000;

/// The hub every subject holds the predicate to.
const HUB: &str = "http://ex.org/hub";

#[when("a reverse list holds more objects than one frame carries, and compute scans it")]
fn hub_scanned(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let sample = SystemId::decode(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0])?;
    let record = 8 + object_of(sample).len() as u64;
    let objects = MAX_FRAME_FLOOR as u64 / record + 1;
    run(async {
        for batch in 0..objects.div_ceil(SUBJECTS_PER_COMMIT) {
            let write = hub_write(batch, objects);
            let _version = commit_at(&address(), PARTITION, &write).await?;
        }
        Ok::<_, TestError>(())
    })?;
    let batches = run(async {
        let target = Address::parse(&address())?;
        let mut client =
            RoleClient::dial_taking(&target, CLUSTER, "compute", SECRET, MAX_FRAME_FLOOR as u32)
                .await?;
        let key = QuadKey::Reverse {
            tenant: TENANT,
            graph: Graph::Default,
            predicate: id_of(&mut client, PREDICATE).await?,
            object: EncodedTerm::id(id_of(&mut client, HUB).await?),
        }
        .to_bytes();
        let scan = plain(ScanAt::Latest, 0, 0, 0, &[alone(key)]);
        let (batches, refused) = answer(&mut client, &scan, true).await?;
        assert!(refused.is_none(), "{refused:?}");
        Ok::<_, TestError>(batches)
    })?;
    w.kernel.batches = batches;
    w.kernel.numbers = vec![objects];
    Ok(())
}

/// The edges of batch `batch` of `objects` subjects to [`HUB`].
fn hub_write(batch: u64, objects: u64) -> Write {
    let mut table = TermTable::new();
    let predicate = table.iri(PREDICATE);
    let object = table.iri(HUB);
    let first = batch * SUBJECTS_PER_COMMIT;
    let changes = (first..objects.min(first + SUBJECTS_PER_COMMIT))
        .map(|at| WriteChange::Insert {
            graph: None,
            subject: table.iri(&format!("http://ex.org/s{at}")),
            predicate,
            object,
        })
        .collect();
    Write::new(table, changes)
}

#[then(
    "the answer takes several frames, each ending between two objects, and compute reads every object once, in the list's order"
)]
fn hub_read(w: &mut Steps) -> TestResult {
    let batches = &w.kernel.batches;
    assert!(batches.len() > 1, "{} frames", batches.len());
    let mut objects = Vec::new();
    for (at, batch) in batches.iter().enumerate() {
        assert!(
            batch.size <= MAX_FRAME_FLOOR,
            "a frame of {} bytes",
            batch.size
        );
        assert_eq!(batch.continues, at + 1 < batches.len(), "frame {at}");
        let (_, value) = batch.entries.first().context("the frame's entry")?;
        // ObjectRecords::read refuses a value cut between two objects' bytes.
        objects.extend(objects_of(value)?);
    }
    let expected = *w.kernel.numbers.first().context("the count")?;
    assert_eq!(objects.len() as u64, expected);
    assert!(objects.is_sorted(), "the list's order");
    objects.dedup();
    assert_eq!(objects.len() as u64, expected, "each object once");
    w.roles = None;
    Ok(())
}

/// The refusal of a Scan of `ranges` arriving at a partition.
pub(crate) async fn refused_scan(
    client: &mut RoleClient,
    ranges: &[(Vec<u8>, Vec<u8>)],
) -> TestResult<Refused> {
    let scan = plain(ScanAt::Latest, 0, 0, 0, ranges);
    let (batches, refused) = answer(client, &scan, false).await?;
    assert!(batches.is_empty(), "before any frame: {batches:?}");
    refused.context("a refusal")
}

pub(crate) fn protocol_naming(refused: &Refused, what: &str) {
    assert_eq!(refused.code, ErrorCode::Protocol, "{refused:?}");
    assert!(refused.message.contains(what), "{refused:?}");
}

#[when("a Scan names a range of the marks keyspace")]
fn marks_range(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let first = key_prefix(Keyspace::Marks, false, TENANT, None, None);
    let refused = run(async { refused_scan(&mut dial().await?, &[(first, Vec::new())]).await })?;
    w.kernel.refused = vec![refused];
    Ok(())
}

#[then("it is refused with a protocol error naming the keyspace")]
fn names_keyspace(w: &mut Steps) -> TestResult {
    protocol_naming(w.kernel.refused.first().context("the refusal")?, "Marks");
    w.roles = None;
    Ok(())
}

#[when("a Scan names a range of the dictionary from one key to another")]
fn wide_dictionary(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let mut keys = [
        term_key(TENANT, "http://ex.org/x"),
        term_key(TENANT, "http://ex.org/y"),
    ];
    keys.sort();
    let [first, past] = keys;
    let refused = run(async { refused_scan(&mut dial().await?, &[(first, past)]).await })?;
    w.kernel.refused = vec![refused];
    Ok(())
}

#[then("it is refused with a protocol error naming the range")]
fn names_range(w: &mut Steps) -> TestResult {
    protocol_naming(w.kernel.refused.first().context("the refusal")?, "range 0");
    w.roles = None;
    Ok(())
}

#[when("a Scan names an epoch the partition has not closed, and the partition then closes it")]
fn waits_for_epoch(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    run(async {
        let mut client = dial().await?;
        let epoch = closed_at(&mut client).await? + 8;
        let version = Version::bound_of(Epoch::new(epoch)?);
        let range = alone(term_key(TENANT, "http://ex.org/x"));
        let scan = plain(ScanAt::ReadVersion, version, epoch, 30_000, &[range]);
        let (batches, refused) = answer(&mut client, &scan, false).await?;
        assert!(refused.is_none(), "{refused:?}");
        w.kernel.numbers = vec![epoch, closed_at(&mut client).await?];
        w.kernel.batches = batches;
        Ok(())
    })
}

#[then("the answer comes after the close, at that epoch")]
fn at_that_epoch(w: &mut Steps) -> TestResult {
    let [epoch, closed] = w.kernel.numbers.as_slice() else {
        return Err(TestError::new("the epochs"));
    };
    assert!(closed >= epoch, "{closed} below {epoch}");
    let batch = w.kernel.batches.first().context("the frame")?;
    assert_eq!(batch.epoch, *epoch);
    w.roles = None;
    Ok(())
}

#[when("a Scan names an epoch the partition does not close before the Scan's deadline")]
fn past_deadline(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    run(async {
        let mut client = dial().await?;
        let epoch = closed_at(&mut client).await? + 1_000_000;
        let version = Version::bound_of(Epoch::new(epoch)?);
        let range = alone(term_key(TENANT, "http://ex.org/x"));
        for deadline_ms in [50, 0] {
            let scan = plain(
                ScanAt::ReadVersion,
                version,
                epoch,
                deadline_ms,
                std::slice::from_ref(&range),
            );
            let (batches, refused) = answer(&mut client, &scan, false).await?;
            w.kernel.batches.extend(batches);
            w.kernel.refused.extend(refused);
        }
        Ok(())
    })
}

#[then("it is refused with code 24 (TimedOut), naming the partition, and returns no entry")]
fn timed_out(w: &mut Steps) -> TestResult {
    assert!(w.kernel.batches.is_empty(), "{:?}", w.kernel.batches);
    assert_eq!(
        w.kernel.refused.len(),
        2,
        "the deadline's, and at once with none"
    );
    for refused in &w.kernel.refused {
        assert_eq!(refused.code, ErrorCode::TimedOut, "{refused:?}");
        assert_eq!(refused.detail, u64::from(PARTITION));
        assert!(
            refused.message.contains(&format!("partition {PARTITION}")),
            "{refused:?}"
        );
    }
    w.roles = None;
    Ok(())
}

/// Ranges of `count` lists nobody holds, one frame each: a run of dictionary ranges would take one.
pub(crate) fn list_ranges(count: u32) -> Vec<(Vec<u8>, Vec<u8>)> {
    (0..count)
        .map(|at| {
            let first = key_prefix(Keyspace::Forward, false, TENANT, None, None);
            alone([first.as_slice(), &at.to_be_bytes()].concat())
        })
        .collect()
}

/// The `scan_stall_ms` the stall's scenario sets: a thread held up by the machine's load between a frame's hand-off and its
/// leaving must not outlast it.
const STALL_MS: u64 = 1_000;

#[when("a Scan's answer takes more frames than the stream's credit, and the reader grants no more")]
fn no_credit(w: &mut Steps) -> TestResult {
    one_partition(w, Some(&STALL_MS.to_string()))?;
    let count = PARTITION_INITIAL_CREDIT + 2;
    let scan = plain(ScanAt::Latest, 0, 0, 0, &list_ranges(count));
    run(async {
        let mut client = dial().await?;
        client.send_only(&scan).await?;
        let mut sequence = EntriesSequence::new(scan.ranges());
        for _ in 0..PARTITION_INITIAL_CREDIT {
            if let Frame::Entries(batch) = next_frame(&mut client, &mut sequence).await? {
                w.kernel.batches.push(batch);
            }
        }
        // Most of the stall passes with nothing on the connection, however long the frames took to come.
        let quiet = Duration::from_millis(STALL_MS * 9 / 10);
        let early = compio::time::timeout(quiet, client.next_frame()).await;
        assert!(early.is_err(), "before `scan_stall_ms` passed");
        if let Frame::Refused(refused) = next_frame(&mut client, &mut sequence).await? {
            w.kernel.refused.push(refused);
        }
        Ok(())
    })
}

#[then(
    "it receives as many frames as its credit, and the Scan is refused with code 24 (TimedOut), naming the partition, once `scan_stall_ms` has passed"
)]
fn stalled(w: &mut Steps) -> TestResult {
    assert_eq!(w.kernel.batches.len(), PARTITION_INITIAL_CREDIT as usize);
    let refused = w.kernel.refused.first().context("the refusal")?;
    assert_eq!(refused.code, ErrorCode::TimedOut, "{refused:?}");
    assert_eq!(refused.detail, u64::from(PARTITION));
    assert!(refused.message.contains("stalled"), "{refused:?}");
    assert!(
        refused.message.contains(&format!("{STALL_MS} ms")),
        "the setting reached the core: {refused:?}"
    );
    w.roles = None;
    Ok(())
}

#[when(
    "a Scan's answer takes more frames than the stream's credit, and the reader grants credit as it reads"
)]
fn with_credit(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let count = PARTITION_INITIAL_CREDIT * 3;
    let scan = plain(ScanAt::Latest, 0, 0, 0, &list_ranges(count));
    let (batches, refused) = run(async { answer(&mut dial().await?, &scan, true).await })?;
    assert!(refused.is_none(), "{refused:?}");
    w.kernel.batches = batches;
    w.kernel.numbers = vec![u64::from(count)];
    Ok(())
}

#[then("it receives every range in order, the last frame carrying `LAST`")]
fn every_range(w: &mut Steps) -> TestResult {
    let count = *w.kernel.numbers.first().context("the count")?;
    let ranges: Vec<u64> = w
        .kernel
        .batches
        .iter()
        .map(|b| u64::from(b.range))
        .collect();
    assert_eq!(ranges, (0..count).collect::<Vec<_>>());
    let lasts: Vec<bool> = w.kernel.batches.iter().map(|b| b.last).collect();
    assert_eq!(lasts.iter().filter(|last| **last).count(), 1);
    assert_eq!(lasts.last(), Some(&true));
    w.roles = None;
    Ok(())
}

#[when("a Scan with `at` 4 names a version above the partition's visibility watermark")]
fn above_watermark(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let subject = subject_of(PARTITION, 1, TENANT)?;
    run(async {
        let _version = commit_at(
            &address(),
            PARTITION,
            &edge_write(&subject, "http://ex.org/o"),
        )
        .await?;
        let mut client = dial().await?;
        let open = closed_at(&mut client).await? + 1_000_000;
        let version = Version::bound_of(Epoch::new(open)?);
        let range = alone(term_key(TENANT, "http://ex.org/x"));
        let scan = plain(ScanAt::Named, version, 0, 0, &[range]);
        let (batches, refused) = answer(&mut client, &scan, false).await?;
        assert!(batches.is_empty(), "{batches:?}");
        w.kernel.refused.extend(refused);
        w.kernel.numbers = vec![watermark().await?];
        Ok(())
    })
}

#[then("it is refused with code 27 (AheadOfWatermark), `detail` the watermark")]
fn names_watermark(w: &mut Steps) -> TestResult {
    let refused = w.kernel.refused.first().context("the refusal")?;
    assert_eq!(refused.code, ErrorCode::AheadOfWatermark, "{refused:?}");
    assert_eq!(Some(&refused.detail), w.kernel.numbers.first());
    assert_ne!(refused.detail, 0, "a commit moved the watermark");
    w.roles = None;
    Ok(())
}

#[when(
    "a partition has closed an epoch, its visibility watermark lies below that epoch's version bound, and a Scan with `at` 4 names that bound"
)]
fn named_on_idle(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let subject = subject_of(PARTITION, 1, TENANT)?;
    run(async {
        let version = commit_at(
            &address(),
            PARTITION,
            &edge_write(&subject, "http://ex.org/o"),
        )
        .await?;
        let mut client = dial().await?;
        closed_by(&mut client, Version::from_u64(version).epoch().get()).await?;
        let key = forward_key(&mut client, &subject).await?;
        let object = id_of(&mut client, "http://ex.org/o").await?;
        let closed = closed_at(&mut client).await?;
        let bound = Version::bound_of(Epoch::new(closed)?);
        let held = watermark().await?;
        assert!(held < bound, "{held} is not below {bound}");
        let scan = plain(ScanAt::Named, bound, 0, 0, &[alone(key)]);
        let (batches, refused) = answer(&mut client, &scan, false).await?;
        assert!(refused.is_none(), "{refused:?}");
        w.kernel.batches = batches;
        w.kernel.bytes = vec![object_of(object)];
        w.kernel.numbers = vec![closed];
        Ok(())
    })
}

#[then("it reads what the partition applied, and is not refused")]
fn reads_applied(w: &mut Steps) -> TestResult {
    let batch = w.kernel.batches.first().context("the frame")?;
    let (_, value) = batch.entries.first().context("the list")?;
    assert_eq!(objects_of(value)?, w.kernel.bytes);
    assert_eq!(
        Some(&batch.epoch),
        w.kernel.numbers.first(),
        "the version's epoch"
    );
    w.roles = None;
    Ok(())
}

#[when(
    "a Scan with `at` 3 has two ranges, and the partition closes another epoch between their frames"
)]
fn across_a_close(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let subject = subject_of(PARTITION, 1, TENANT)?;
    run(async {
        let first = commit_at(
            &address(),
            PARTITION,
            &edge_write(&subject, "http://ex.org/o1"),
        )
        .await?;
        let mut client = dial().await?;
        closed_by(&mut client, Version::from_u64(first).epoch().get()).await?;
        let key = forward_key(&mut client, &subject).await?;
        let kept = id_of(&mut client, "http://ex.org/o1").await?;
        // The credit stops the answer after these, before its last range.
        let mut ranges = list_ranges(PARTITION_INITIAL_CREDIT);
        ranges.push(alone(key));
        let scan = plain(ScanAt::LatestClosed, 0, 0, 0, &ranges);
        client.send_only(&scan).await?;
        let stream = client.last_stream();
        let mut sequence = EntriesSequence::new(scan.ranges());
        let mut batches = Vec::new();
        for _ in 0..PARTITION_INITIAL_CREDIT {
            if let Frame::Entries(batch) = next_frame(&mut client, &mut sequence).await? {
                batches.push(batch);
            }
        }
        let chosen = batches.first().context("the first frame")?.epoch;
        let second = commit_at(
            &address(),
            PARTITION,
            &edge_write(&subject, "http://ex.org/o2"),
        )
        .await?;
        let mut closes = dial().await?;
        closed_by(
            &mut closes,
            Version::from_u64(second).epoch().get().max(chosen + 1),
        )
        .await?;
        client.send_on(stream, &Credit::new(1)).await?;
        if let Frame::Entries(batch) = next_frame(&mut client, &mut sequence).await? {
            batches.push(batch);
        }
        w.kernel.batches = batches;
        w.kernel.bytes = vec![object_of(kept)];
        w.kernel.numbers = vec![chosen];
        Ok(())
    })
}

#[then("both ranges read the version chosen first, and every frame names its epoch")]
fn one_version(w: &mut Steps) -> TestResult {
    let chosen = w.kernel.numbers.first().context("the epoch")?;
    assert_eq!(w.kernel.batches.len() as u32, PARTITION_INITIAL_CREDIT + 1);
    assert!(
        w.kernel.batches.iter().all(|batch| batch.epoch == *chosen),
        "{:?}",
        w.kernel.batches
    );
    let last = w.kernel.batches.last().context("the list's frame")?;
    assert!(last.last, "the answer's last frame");
    let (_, value) = last.entries.first().context("the list")?;
    assert_eq!(
        objects_of(value)?,
        w.kernel.bytes,
        "no commit after the version chosen"
    );
    w.roles = None;
    Ok(())
}

/// A Scan naming transaction `identity`, reading at `version`.
pub(crate) fn of_transaction(identity: u8, version: u64, principal: u64) -> Scan {
    Scan::new(
        ScanAt::ReadVersion,
        version,
        0,
        principal,
        [identity; 16],
        0,
        0,
    )
}

/// Long keys, so a transaction's read set fills with few ranges, each a range of its own: `start` onwards.
fn long_ranges(start: u8, count: u8) -> Vec<(Vec<u8>, Vec<u8>)> {
    const KEY: usize = 60_000;
    (start..start + count)
        .map(|at| {
            let mut first = key_prefix(Keyspace::Forward, false, TENANT, None, None);
            first.resize(KEY, at);
            let past = [first.as_slice(), &[1]].concat();
            (first, past)
        })
        .collect()
}

#[when("a Scan's span has no room in the transaction's read set")]
fn no_room(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    run(async {
        let mut client = dial().await?;
        let reply = client.request(&bind_of(3, PRINCIPAL)).await?;
        let read = BindView::from_frame(&reply.view()?)?.read_version();
        drop(reply);
        let bind_stream = client.last_stream();
        let mut refusals = Vec::new();
        for scan_at in 0u8..8 {
            let mut scan = of_transaction(3, read, PRINCIPAL);
            let ranges = long_ranges(scan_at * 4 + 1, 4);
            for (first, past) in &ranges {
                scan.push_range(ScanRange::new(PARTITION, first, past));
            }
            let (_, refused) = answer(&mut client, &scan, true).await?;
            if let Some(refused) = refused {
                refusals.push(refused);
                break;
            }
        }
        let scan_stream = client.last_stream();
        let mut sequence = EntriesSequence::new([]);
        if let Frame::Refused(refused) = next_frame(&mut client, &mut sequence).await? {
            refusals.push(refused);
        }
        // The Bind stream has ended, so a frame on it is late; the transaction's next request is a Scan naming it.
        let mut next = of_transaction(3, read, PRINCIPAL);
        for (first, past) in &long_ranges(1, 1) {
            next.push_range(ScanRange::new(PARTITION, first, past));
        }
        client.send_only(&next).await?;
        if let Frame::Refused(refused) = next_frame(&mut client, &mut sequence).await? {
            refusals.push(refused);
        }
        w.kernel.numbers = vec![bind_stream, scan_stream];
        w.kernel.refused = refusals;
        Ok(())
    })
}

#[then(
    "the Scan's stream and the Bind stream each carry code 6 (TooLarge), and the transaction's next request is refused"
)]
fn both_streams(w: &mut Steps) -> TestResult {
    let [first, second, next] = w.kernel.refused.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.kernel.refused)));
    };
    let mut streams = [first.stream, second.stream];
    streams.sort_unstable();
    assert_eq!(streams.as_slice(), w.kernel.numbers.as_slice());
    for refused in [first, second] {
        assert_eq!(refused.code, ErrorCode::TooLarge, "{refused:?}");
        assert_eq!(refused.detail, MAX_FRAME_SIZE as u64, "the limit in bytes");
    }
    assert_eq!(next.code, ErrorCode::Protocol, "{next:?}");
    assert_eq!(next.detail, DETAIL_TRANSACTION_NOT_HELD, "{next:?}");
    w.roles = None;
    Ok(())
}

#[when("compute sends Closed naming the partition at its role address")]
fn closed_asked(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    run(async {
        let mut client = dial().await?;
        closed_by(&mut client, 1).await?;
        w.kernel.numbers = vec![closed_at(&mut client).await?];
        client.send_only(&Closed::request(&[PARTITION + 1])).await?;
        let mut sequence = EntriesSequence::new([]);
        if let Frame::Refused(refused) = next_frame(&mut client, &mut sequence).await? {
            w.kernel.refused.push(refused);
        }
        Ok(())
    })
}

#[then("the reply holds the partition's latest closed epoch")]
fn closed_held(w: &mut Steps) -> TestResult {
    assert!(*w.kernel.numbers.first().context("the epoch")? >= 1);
    let refused = w.kernel.refused.first().context("another partition's")?;
    protocol_naming(refused, &format!("{}", PARTITION + 1));
    w.roles = None;
    Ok(())
}

#[when(
    "Scans name a first key too short for a tenant, a past key beyond the tenant, and the first key past it"
)]
fn tenant_rules(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let subject = subject_of(PARTITION, 1, TENANT)?;
    run(async {
        let _version = commit_at(
            &address(),
            PARTITION,
            &edge_write(&subject, "http://ex.org/o"),
        )
        .await?;
        let mut client = dial().await?;
        let first = key_prefix(Keyspace::Forward, false, TENANT, None, None);
        let past = key_prefix(Keyspace::Forward, false, TENANT + 1, None, None);
        let short = (vec![Keyspace::Forward.byte(), 0, 1], Vec::new());
        w.kernel
            .refused
            .push(refused_scan(&mut client, &[short]).await?);
        let beyond = [past.as_slice(), &[0]].concat();
        w.kernel
            .refused
            .push(refused_scan(&mut client, &[(first.clone(), beyond)]).await?);
        let scan = plain(ScanAt::Latest, 0, 0, 0, &[(first, past)]);
        let (batches, refused) = answer(&mut client, &scan, false).await?;
        assert!(refused.is_none(), "{refused:?}");
        w.kernel.batches = batches;
        Ok(())
    })
}

#[then(
    "the first two are refused with a protocol error naming the range, and the third reads to the tenant's end"
)]
fn tenant_ruled(w: &mut Steps) -> TestResult {
    for refused in &w.kernel.refused {
        protocol_naming(refused, "range 0");
    }
    assert_eq!(w.kernel.refused.len(), 2);
    let batch = w.kernel.batches.first().context("the frame")?;
    assert_eq!(batch.entries.len(), 1);
    assert_eq!(batch.end, EntriesEnd::Ended.number());
    w.roles = None;
    Ok(())
}

#[when("a Scan's limit equals what a range holds, and is one below what another holds")]
fn exact_limits(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    run(async {
        for at in ["http://ex.org/l1", "http://ex.org/l2"] {
            let _version =
                commit_at(&address(), PARTITION, &edge_write(at, "http://ex.org/o")).await?;
        }
        let mut client = dial().await?;
        let first = key_prefix(Keyspace::Forward, false, TENANT, None, None);
        for limit in [2, 1] {
            let mut scan = Scan::new(ScanAt::Latest, 0, 0, PRINCIPAL, NO_TRANSACTION, limit, 0);
            let dictionary = alone(term_key(TENANT, "http://ex.org/l1"));
            scan.push_range(ScanRange::new(PARTITION, &first, b""));
            scan.push_range(ScanRange::new(PARTITION, &dictionary.0, &dictionary.1));
            let (batches, refused) = answer(&mut client, &scan, true).await?;
            assert!(refused.is_none(), "{refused:?}");
            w.kernel.batches.extend(batches);
        }
        Ok(())
    })
}

#[then(
    "the first range ends with `end` 1, a one-key range of the dictionary too, and the other with `end` 2"
)]
fn exact_ends(w: &mut Steps) -> TestResult {
    let ends: Vec<(u32, u8, usize)> = w
        .kernel
        .batches
        .iter()
        .map(|b| (b.range, b.end, b.entries.len()))
        .collect();
    let (ended, limit) = (EntriesEnd::Ended.number(), EntriesEnd::Limit.number());
    assert_eq!(
        ends,
        [(0, ended, 2), (1, ended, 1), (0, limit, 1), (1, ended, 1)]
    );
    w.roles = None;
    Ok(())
}

/// Sends `write` on the Bind stream `stream` of `client`, and reads the Held that answers it.
pub(crate) async fn write_held(client: &mut RoleClient, stream: u64, write: &Write) -> TestResult {
    client.send_on(stream, write).await?;
    let held = client.next_frame().await?;
    let view = held.view()?;
    assert_eq!(view.header().stream(), stream);
    let _answer: Command = view.command().answering(Command::Write)?;
    if view.command() == Command::Held {
        return Ok(());
    }
    Err(TestError::new(format!(
        "command {}",
        view.command().number()
    )))
}

/// Commits or fails on `stream` of `client`: the frame that answers, a refusal if it is one.
pub(crate) async fn commit_answer(
    client: &mut RoleClient,
    stream: u64,
) -> TestResult<Option<Refused>> {
    client.send_on(stream, &Commit::new([0; 16], 0, 0)).await?;
    let mut sequence = EntriesSequence::new([]);
    match next_frame(client, &mut sequence).await? {
        Frame::Refused(refused) => Ok(Some(refused)),
        Frame::Entries(_) | Frame::Other(_) => Ok(None),
    }
}

#[when(
    "a transaction writes `<a> <p> <b>`, a Scan naming it reads the list, a second transaction commits into that list, and the first commits"
)]
fn own_write_read(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let (a, b, c) = ("http://ex.org/a", "http://ex.org/b", "http://ex.org/c");
    run(async {
        // `<a>` and `<p>` are held, so what the later commit conflicts with is the list alone.
        let _held = commit_at(&address(), PARTITION, &edge_write(a, "http://ex.org/z")).await?;
        let mut client = dial().await?;
        let reply = client.request(&bind_of(4, PRINCIPAL)).await?;
        let read = BindView::from_frame(&reply.view()?)?.read_version();
        drop(reply);
        let bind_stream = client.last_stream();
        write_held(&mut client, bind_stream, &edge_write(a, b)).await?;
        let mut terms = TermTable::new();
        for iri in [PREDICATE, a, b] {
            let _at = terms.iri(iri);
        }
        let mut scan = of_transaction(4, read, PRINCIPAL).with_terms(terms);
        let keys = [PREDICATE, a, b].map(|iri| alone(term_key(TENANT, iri)));
        for (first, past) in &keys {
            scan.push_range(ScanRange::new(PARTITION, first, past));
        }
        let (ids, refused) = answer(&mut client, &scan, false).await?;
        assert!(refused.is_none(), "{refused:?}");
        let id = |at: usize| -> TestResult<SystemId> {
            let (_, value) = ids
                .first()
                .and_then(|batch| batch.entries.get(at))
                .context("an ID")?;
            Ok(SystemId::decode(value)?)
        };
        let key = QuadKey::Forward {
            tenant: TENANT,
            graph: Graph::Default,
            predicate: id(0)?,
            subject: id(1)?,
        }
        .to_bytes();
        let mut list = of_transaction(4, read, PRINCIPAL);
        let (first, past) = alone(key.clone());
        list.push_range(ScanRange::new(PARTITION, &first, &past));
        let (own, list_refused) = answer(&mut client, &list, false).await?;
        assert!(list_refused.is_none(), "{list_refused:?}");
        let other = plain(ScanAt::Latest, 0, 0, 0, &[alone(key)]);
        let (seen, _) = answer(&mut dial().await?, &other, false).await?;
        let _second = commit_at(&address(), PARTITION, &edge_write(a, c)).await?;
        w.kernel
            .refused
            .extend(commit_answer(&mut client, bind_stream).await?);
        w.kernel.bytes = vec![object_of(id(2)?)];
        w.kernel.batches = own;
        w.kernel.batches.extend(seen);
        Ok(())
    })
}

#[then(
    "the Scan's frames hold `<b>`, a Scan naming no transaction does not, and the first commit aborts with a conflict of kind Range"
)]
fn own_write_seen(w: &mut Steps) -> TestResult {
    let [own, seen] = w.kernel.batches.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.kernel.batches)));
    };
    let (_, value) = own.entries.first().context("the list")?;
    let b = w.kernel.bytes.first().context("<b>")?;
    assert!(objects_of(value)?.contains(b), "its own write");
    let (_, others) = seen.entries.first().context("the list")?;
    assert!(!objects_of(others)?.contains(b), "nobody else's");
    let refused = w.kernel.refused.first().context("the abort")?;
    assert_eq!(refused.code, ErrorCode::Conflict, "{refused:?}");
    assert_eq!(
        refused.detail,
        ultragraphdb_protocol_wire::ConflictKind::Range.number()
    );
    w.roles = None;
    Ok(())
}

#[when(
    "Scans name a transaction with a wrong read version, a wrong principal, another tenant, a referrer set, a key not its term's, or an identity it does not hold"
)]
fn wrong_transaction_scans(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    run(async {
        let mut client = dial().await?;
        let reply = client.request(&bind_of(5, PRINCIPAL)).await?;
        let read = BindView::from_frame(&reply.view()?)?.read_version();
        drop(reply);
        let term = alone(term_key(TENANT, "http://ex.org/t"));
        let forward = key_prefix(Keyspace::Forward, false, TENANT, None, None);
        let foreign = key_prefix(Keyspace::Forward, false, TENANT + 1, None, None);
        let referrers = key_prefix(Keyspace::Referrers, false, TENANT, None, None);
        let one = |first: &Vec<u8>, version: u64, principal: u64, identity: u8| {
            let mut scan = of_transaction(identity, version, principal);
            scan.push_range(ScanRange::new(PARTITION, first, b""));
            scan
        };
        let mut terms = TermTable::new();
        let _at = terms.iri("http://ex.org/another");
        let mut named = of_transaction(5, read, PRINCIPAL).with_terms(terms);
        named.push_range(ScanRange::new(PARTITION, &term.0, &term.1));
        let scans = [
            one(&forward, read + 1, PRINCIPAL, 5),
            one(&forward, read, PRINCIPAL + 1, 5),
            one(&foreign, read, PRINCIPAL, 5),
            one(&referrers, read, PRINCIPAL, 5),
            named,
            one(&forward, read, PRINCIPAL, 6),
        ];
        for scan in &scans {
            let (batches, refused) = answer(&mut client, scan, false).await?;
            assert!(batches.is_empty(), "{batches:?}");
            w.kernel.refused.extend(refused);
        }
        let held = one(&forward, read, PRINCIPAL, 5);
        let (batches, refused) = answer(&mut client, &held, false).await?;
        assert!(refused.is_none(), "{refused:?}");
        w.kernel.batches = batches;
        Ok(())
    })
}

#[then("each is refused with a protocol error naming what is wrong, and the transaction goes on")]
fn wrong_named(w: &mut Steps) -> TestResult {
    let names = [
        "read_version",
        "principal",
        "tenant",
        "referrer",
        "scan term",
        "does not hold",
    ];
    assert_eq!(
        w.kernel.refused.len(),
        names.len(),
        "{:?}",
        w.kernel.refused
    );
    for (refused, name) in w.kernel.refused.iter().zip(names) {
        protocol_naming(refused, name);
    }
    assert_eq!(w.kernel.batches.len(), 1, "the transaction read on");
    w.roles = None;
    Ok(())
}

#[when("a Bind names a transaction the partition holds")]
fn bind_twice(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    run(async {
        let mut client = dial().await?;
        drop(client.request(&bind_of(6, PRINCIPAL)).await?);
        client.send_only(&bind_of(6, PRINCIPAL)).await?;
        let mut sequence = EntriesSequence::new([]);
        if let Frame::Refused(refused) = next_frame(&mut client, &mut sequence).await? {
            w.kernel.refused.push(refused);
        }
        Ok(())
    })
}

#[then("it is refused with a protocol error naming the transaction")]
fn bind_refused(w: &mut Steps) -> TestResult {
    let refused = w.kernel.refused.first().context("the refusal")?;
    let identity = format!("{:032x}", u128::from_be_bytes([6; 16]));
    protocol_naming(refused, &identity);
    w.roles = None;
    Ok(())
}

/// Binds a transaction at `client`, partition 1 of two processes' four: its stream and version.
pub(crate) async fn bind_first(client: &mut RoleClient, identity: u8) -> TestResult<(u64, u64)> {
    let bind = Bind::new(
        1,
        BindKind::Transaction,
        TENANT,
        0,
        PRINCIPAL,
        [identity; 16],
    );
    let reply = client.request(&bind).await?;
    let read = BindView::from_frame(&reply.view()?)?.read_version();
    Ok((client.last_stream(), read))
}

/// A Scan of `iri` as transaction `identity`'s term, at `version`.
pub(crate) fn term_scan(identity: u8, version: u64, iri: &str) -> Scan {
    let mut terms = TermTable::new();
    let _at = terms.iri(iri);
    let mut scan = of_transaction(identity, version, PRINCIPAL).with_terms(terms);
    let (first, past) = alone(term_key(TENANT, iri));
    scan.push_range(ScanRange::new(1, &first, &past));
    scan
}

#[when("a transaction on partition 1 names in a Scan an IRI a partition of another process owns")]
fn foreign_from_home(w: &mut Steps) -> TestResult {
    two_processes(w)?;
    let iri = subject_of(3, 4, TENANT)?;
    run(async {
        let mut client = dial_at("tcp://127.0.0.1:17200").await?;
        let (_, read) = bind_first(&mut client, 7).await?;
        let (batches, refused) = answer(&mut client, &term_scan(7, read, &iri), false).await?;
        assert!(refused.is_none(), "{refused:?}");
        w.kernel.batches = batches;
        let mut home = dial_at("tcp://127.0.0.1:17300").await?;
        let mut asked = Scan::new(ScanAt::Latest, 0, 0, PRINCIPAL, NO_TRANSACTION, 0, 0);
        let (first, past) = alone(term_key(TENANT, &iri));
        asked.push_range(ScanRange::new(3, &first, &past));
        let (held, _) = answer(&mut home, &asked, false).await?;
        let (_, id) = held
            .first()
            .and_then(|batch| batch.entries.first())
            .context("the home's entry")?;
        w.kernel.bytes = vec![id.clone()];
        Ok(())
    })
}

#[then("the range answers the ID that process's dictionary holds")]
fn home_id(w: &mut Steps) -> TestResult {
    let batch = w.kernel.batches.first().context("the frame")?;
    let (_, id) = batch.entries.first().context("the ID")?;
    assert_eq!(Some(id), w.kernel.bytes.first());
    w.roles = None;
    Ok(())
}

/// The refusal of what `sent` names, sent on a stream of its own: the next frame `client` reads.
async fn refusal_of(client: &mut RoleClient, sent: Refusing<'_>) -> TestResult<Refused> {
    match sent {
        Refusing::Scan(scan) => client.send_only(scan).await?,
        Refusing::Recall(recall) => client.send_only(recall).await?,
    }
    let mut sequence = EntriesSequence::new([]);
    match next_frame(client, &mut sequence).await? {
        Frame::Refused(refused) => Ok(refused),
        Frame::Entries(batch) => Err(TestError::new(format!("a frame: {batch:?}"))),
        Frame::Other(command) => Err(TestError::new(format!("command {}", command.number()))),
    }
}

/// What [`refusal_of`] sends.
enum Refusing<'a> {
    Scan(&'a Scan),
    Recall(&'a Recall),
}

#[when("a Scan and a Recall name a partition the role address does not serve")]
fn other_partition(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    run(async {
        let mut client = dial().await?;
        let (first, past) = alone(term_key(TENANT, "http://ex.org/x"));
        let mut scan = Scan::new(ScanAt::Latest, 0, 0, PRINCIPAL, NO_TRANSACTION, 0, 0);
        scan.push_range(ScanRange::new(PARTITION + 1, &first, &past));
        w.kernel
            .refused
            .push(refusal_of(&mut client, Refusing::Scan(&scan)).await?);
        let recall = Recall::new(PARTITION + 1, [1; 16], 1);
        w.kernel
            .refused
            .push(refusal_of(&mut client, Refusing::Recall(&recall)).await?);
        Ok(())
    })
}

#[then("each is refused with a protocol error naming the partition")]
fn names_partition(w: &mut Steps) -> TestResult {
    assert_eq!(w.kernel.refused.len(), 2, "{:?}", w.kernel.refused);
    for refused in &w.kernel.refused {
        protocol_naming(refused, &format!("not {}", PARTITION + 1));
    }
    w.roles = None;
    Ok(())
}

#[when(
    "Scans hold a valid range and then one of the marks keyspace, of a split part, of an unknown keyspace, of the dictionary with no past key, of the referrer sets with a first key too short, or of the referrer sets past their tenant"
)]
fn bad_second_ranges(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let referrers = key_prefix(Keyspace::Referrers, false, TENANT, None, None);
    let beyond = key_prefix(Keyspace::Referrers, false, TENANT + 1, None, None);
    let unknown = (0..=u8::MAX)
        .find(|byte| Keyspace::from_byte(*byte).is_none())
        .context("a byte naming no keyspace")?;
    let cases = [
        (
            key_prefix(Keyspace::Marks, false, TENANT, None, None),
            Vec::new(),
        ),
        (
            key_prefix(Keyspace::Forward, true, TENANT, None, None),
            Vec::new(),
        ),
        (vec![unknown; 9], Vec::new()),
        (term_key(TENANT, "http://ex.org/y"), Vec::new()),
        (vec![Keyspace::Referrers.byte(), 0, 1], Vec::new()),
        (referrers, [beyond.as_slice(), &[0]].concat()),
    ];
    let valid = alone(term_key(TENANT, "http://ex.org/x"));
    run(async {
        let mut client = dial().await?;
        for bad in cases {
            let ranges = [valid.clone(), bad];
            w.kernel
                .refused
                .push(refused_scan(&mut client, &ranges).await?);
        }
        Ok(())
    })?;
    w.kernel.bytes = vec![vec![unknown]];
    Ok(())
}

#[then(
    "each is refused with a protocol error naming what is wrong, and no frame precedes the refusal"
)]
fn each_named(w: &mut Steps) -> TestResult {
    let unknown = *w
        .kernel
        .bytes
        .first()
        .and_then(|byte| byte.first())
        .context("the byte")?;
    let names = [
        "Marks".to_string(),
        "split part".to_string(),
        format!("{unknown:#04x}"),
        "range 1".to_string(),
        "range 1".to_string(),
        "range 1".to_string(),
    ];
    assert_eq!(
        w.kernel.refused.len(),
        names.len(),
        "{:?}",
        w.kernel.refused
    );
    for (refused, name) in w.kernel.refused.iter().zip(names) {
        protocol_naming(refused, &name);
    }
    w.roles = None;
    Ok(())
}

#[when("the dictionary or the statements hold a value too large for the receiver's `max_frame`")]
fn too_large_value(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let lexical = "x".repeat(MAX_FRAME_FLOOR);
    let mut table = TermTable::new();
    let subject = table.iri("http://ex.org/long");
    let predicate = table.iri(PREDICATE);
    let object = table.intern(RequestTerm::Literal {
        lexical: lexical.clone(),
        datatype: None,
    })?;
    let write = Write::new(
        table,
        vec![WriteChange::Insert {
            graph: None,
            subject,
            predicate,
            object,
        }],
    );
    let key = literal_key(TENANT, &Hash128::of(lexical.as_bytes()));
    run(async {
        let _version = commit_at(&address(), PARTITION, &write).await?;
        let target = Address::parse(&address())?;
        let taking = MAX_FRAME_FLOOR as u32;
        let mut small =
            RoleClient::dial_taking(&target, CLUSTER, "compute", SECRET, taking).await?;
        let scan = plain(ScanAt::Latest, 0, 0, 0, std::slice::from_ref(&alone(key)));
        w.kernel
            .refused
            .push(refusal_of(&mut small, Refusing::Scan(&scan)).await?);
        drop(small);
        w.kernel
            .refused
            .push(crate::scan_ranges_steps::too_large_record().await?);
        Ok(())
    })
}

#[then("a Scan of it is refused with code 6 (TooLarge), `detail` that `max_frame`")]
fn too_large_named(w: &mut Steps) -> TestResult {
    assert_eq!(w.kernel.refused.len(), 2, "{:?}", w.kernel.refused);
    for refused in &w.kernel.refused {
        assert_eq!(refused.code, ErrorCode::TooLarge, "{refused:?}");
        assert_eq!(refused.detail, MAX_FRAME_FLOOR as u64);
    }
    w.roles = None;
    Ok(())
}

#[when("one Scan of a connection holds no credit and another takes its credit as it reads")]
fn two_scans_open(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let (stopped, running) = (PARTITION_INITIAL_CREDIT + 1, PARTITION_INITIAL_CREDIT * 3);
    run(async {
        let mut client = dial().await?;
        let idle = plain(ScanAt::Latest, 0, 0, 0, &list_ranges(stopped));
        client.send_only(&idle).await?;
        let idle_stream = client.last_stream();
        let read = plain(ScanAt::Latest, 0, 0, 0, &list_ranges(running));
        client.send_only(&read).await?;
        let read_stream = client.last_stream();
        let (mut idle_seq, mut read_seq) = (
            EntriesSequence::new(idle.ranges()),
            EntriesSequence::new(read.ranges()),
        );
        loop {
            let frame = client.next_frame().await?;
            let stream = frame.view()?.header().stream();
            let sequence = if stream == idle_stream {
                &mut idle_seq
            } else {
                &mut read_seq
            };
            let Frame::Entries(batch) = decoded(&frame, sequence)? else {
                return Err(TestError::new("a frame of neither Scan's"));
            };
            drop(frame);
            let last = batch.last;
            w.kernel.batches.push(batch);
            if stream == read_stream {
                if last {
                    break;
                }
                client.send_on(read_stream, &Credit::new(1)).await?;
            }
        }
        w.kernel.numbers = vec![idle_stream, read_stream, u64::from(running)];
        Ok(())
    })
}

#[then("the second reaches `LAST` while the first takes no more frames")]
fn other_ran(w: &mut Steps) -> TestResult {
    let [idle, read, running] = w.kernel.numbers.as_slice() else {
        return Err(TestError::new("the streams"));
    };
    let of = |stream: u64| w.kernel.batches.iter().filter(move |b| b.stream == stream);
    assert_eq!(of(*read).count() as u64, *running);
    assert!(of(*read).next_back().is_some_and(|batch| batch.last));
    assert!(
        of(*idle).count() <= PARTITION_INITIAL_CREDIT as usize,
        "its credit bounds it"
    );
    assert!(of(*idle).all(|batch| !batch.last), "it never ended");
    w.roles = None;
    Ok(())
}

/// More emptied lists than a turn of the partition reads.
const EMPTIED: u32 = 300;

/// A Write changing `http://ex.org/e{at}` `<p>` `<o>` for each of the emptied lists.
fn emptied_write(change: impl Fn(u32, u32, u32) -> WriteChange) -> Write {
    let mut table = TermTable::new();
    let predicate = table.iri(PREDICATE);
    let object = table.iri("http://ex.org/o");
    let changes = (0..EMPTIED)
        .map(|at| {
            change(
                table.iri(&format!("http://ex.org/e{at}")),
                predicate,
                object,
            )
        })
        .collect();
    Write::new(table, changes)
}

#[when(
    "a range holds more emptied lists than a turn reads, and a list holding an object after them"
)]
fn emptied_range(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let inserted = emptied_write(|subject, predicate, object| WriteChange::Insert {
        graph: None,
        subject,
        predicate,
        object,
    });
    let deleted = emptied_write(|subject, predicate, object| WriteChange::Delete {
        graph: None,
        subject,
        predicate,
        object,
    });
    run(async {
        let _held = commit_at(&address(), PARTITION, &inserted).await?;
        let _gone = commit_at(&address(), PARTITION, &deleted).await?;
        let _kept = commit_at(
            &address(),
            PARTITION,
            &edge_write("http://ex.org/kept", "http://ex.org/o"),
        )
        .await?;
        let mut client = dial().await?;
        let kept = forward_key(&mut client, "http://ex.org/kept").await?;
        let first = key_prefix(Keyspace::Forward, false, TENANT, None, None);
        let scan = plain(ScanAt::Latest, 0, 0, 0, &[(first, Vec::new())]);
        let (batches, refused) = answer(&mut client, &scan, true).await?;
        assert!(refused.is_none(), "{refused:?}");
        w.kernel.batches = batches;
        w.kernel.bytes = vec![kept];
        Ok(())
    })
}

#[then("the answer holds that list alone, and the range ends with `end` 1")]
fn emptied_answer(w: &mut Steps) -> TestResult {
    let keys: Vec<&Vec<u8>> = w
        .kernel
        .batches
        .iter()
        .flat_map(|batch| batch.entries.iter().map(|(key, _)| key))
        .collect();
    assert_eq!(keys, w.kernel.bytes.iter().collect::<Vec<_>>());
    let last = w.kernel.batches.last().context("a frame")?;
    assert_eq!(last.end, EntriesEnd::Ended.number());
    assert!(last.last);
    w.roles = None;
    Ok(())
}

#[when("a Scan of a transaction waits for credit, and the transaction aborts")]
fn aborted_under_scan(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let count = PARTITION_INITIAL_CREDIT + 1;
    run(async {
        let mut client = dial().await?;
        let reply = client.request(&bind_of(21, PRINCIPAL)).await?;
        let read = BindView::from_frame(&reply.view()?)?.read_version();
        drop(reply);
        let bind_stream = client.last_stream();
        let mut scan = of_transaction(21, read, PRINCIPAL);
        for (first, past) in &list_ranges(count) {
            scan.push_range(ScanRange::new(PARTITION, first, past));
        }
        client.send_only(&scan).await?;
        let scan_stream = client.last_stream();
        let mut sequence = EntriesSequence::new(scan.ranges());
        for _ in 0..PARTITION_INITIAL_CREDIT {
            if let Frame::Entries(batch) = next_frame(&mut client, &mut sequence).await? {
                w.kernel.batches.push(batch);
            }
        }
        client.send_on(bind_stream, &Abort).await?;
        if let Frame::Refused(refused) = next_frame(&mut client, &mut sequence).await? {
            w.kernel.refused.push(refused);
        }
        w.kernel.numbers = vec![scan_stream];
        Ok(())
    })
}

#[then("the Scan is refused at once with Error 1, `detail` 1, as one the partition does not hold")]
fn ended_with_it(w: &mut Steps) -> TestResult {
    assert_eq!(w.kernel.batches.len(), PARTITION_INITIAL_CREDIT as usize);
    let refused = w.kernel.refused.first().context("the refusal")?;
    protocol_naming(refused, "does not hold");
    assert_eq!(refused.detail, 1, "{refused:?}");
    assert_eq!(Some(&refused.stream), w.kernel.numbers.first());
    w.roles = None;
    Ok(())
}

/// An IRI partition 3 of 4 owns, `at` choosing among them.
pub(crate) fn homed_at_three(at: u32) -> TestResult<String> {
    let map = PlacementMap::even(4, NodeId::new(1))?;
    (0..100_000)
        .map(|n| format!("http://ex.org/h{at}-{n}"))
        .find(|iri| {
            iri_bucket(TENANT, iri)
                .is_some_and(|bucket| map.partition_of(bucket, Epoch::ZERO).get() == 3)
        })
        .context("an IRI of partition 3")
}

#[when("two Scans of a transaction overlap, each naming an IRI of a home that has not answered")]
fn overlapping_scans(w: &mut Steps) -> TestResult {
    one_process_of_two(w)?;
    let (x, y) = (homed_at_three(1)?, homed_at_three(2)?);
    let script = HomeScript::holding();
    run(async {
        play_home(&script).await?;
        let mut client = dial_at("tcp://127.0.0.1:17200").await?;
        let (_, read) = bind_first(&mut client, 22).await?;
        client.send_only(&term_scan(22, read, &x)).await?;
        script.asked().await?;
        client.send_only(&term_scan(22, read, &y)).await?;
        // The second is taken while the first's answer is held, and nothing refuses it.
        let held = Duration::from_millis(50);
        let early = compio::time::timeout(held, client.next_frame()).await;
        assert!(early.is_err(), "no frame before the home answers");
        script.release();
        for _ in 0..2 {
            // The two Scans answer in either order, each of one key of the dictionary.
            let first = [Keyspace::Dictionary.byte()];
            let mut sequence = EntriesSequence::new([ScanRange::new(PARTITION, &first, &[])]);
            match next_frame(&mut client, &mut sequence).await? {
                Frame::Entries(batch) => w.kernel.batches.push(batch),
                Frame::Refused(refused) => w.kernel.refused.push(refused),
                Frame::Other(command) => {
                    return Err(TestError::new(format!("command {}", command.number())));
                }
            }
        }
        Ok::<_, TestError>(())
    })?;
    w.kernel.bytes = [&x, &y]
        .iter()
        .map(|iri| Ok(played_id(iri)?.to_be_bytes().to_vec()))
        .collect::<TestResult<_>>()?;
    Ok(())
}

#[then("neither is refused, and each range answers the ID its home gave")]
fn both_answered(w: &mut Steps) -> TestResult {
    assert!(w.kernel.refused.is_empty(), "{:?}", w.kernel.refused);
    let ids: Vec<&Vec<u8>> = w
        .kernel
        .batches
        .iter()
        .flat_map(|batch| batch.entries.iter().map(|(_, id)| id))
        .collect();
    assert_eq!(ids, w.kernel.bytes.iter().collect::<Vec<_>>());
    w.roles = None;
    Ok(())
}

#[when("a transaction's Write waits on a home's answer, and a Scan of the transaction arrives")]
fn scan_during_write(w: &mut Steps) -> TestResult {
    one_process_of_two(w)?;
    let (subject, object) = (subject_of(1, 4, TENANT)?, homed_at_three(3)?);
    let script = HomeScript::holding();
    run(async {
        play_home(&script).await?;
        let mut client = dial_at("tcp://127.0.0.1:17200").await?;
        let (bind_stream, read) = bind_first(&mut client, 23).await?;
        client
            .send_on(bind_stream, &edge_write(&subject, &object))
            .await?;
        script.asked().await?;
        // The frontend holds the Bind stream's connection and compute dials its own, as this Scan does.
        let mut compute = dial_at("tcp://127.0.0.1:17200").await?;
        let scan = term_scan(23, read, &subject);
        w.kernel
            .refused
            .push(refusal_of(&mut compute, Refusing::Scan(&scan)).await?);
        script.release();
        Ok(())
    })
}

#[then("the Scan is refused with a protocol error naming the write")]
fn names_the_write(w: &mut Steps) -> TestResult {
    protocol_naming(w.kernel.refused.first().context("the refusal")?, "write");
    w.roles = None;
    Ok(())
}

#[when(
    "a transaction names in a Scan a skolem IRI no dictionary holds, homed on another partition"
)]
fn foreign_skolem(w: &mut Steps) -> TestResult {
    two_processes(w)?;
    let homed = subject_of(3, 4, TENANT)?;
    let bucket = iri_bucket(TENANT, &homed).context("a bucket")?;
    let skolem = Names::new(tenant_base(TENANT))?.skolem_iri(bucket, 1);
    run(async {
        let mut client = dial_at("tcp://127.0.0.1:17200").await?;
        let (_, read) = bind_first(&mut client, 8).await?;
        // A second Scan of the IRI finds what the first did, asking no one, and the transaction still reads.
        for _ in 0..2 {
            let scan = term_scan(8, read, skolem.as_str());
            let (batches, refused) = answer(&mut client, &scan, false).await?;
            assert!(refused.is_none(), "{refused:?}");
            w.kernel.batches.extend(batches);
        }
        Ok(())
    })
}

#[then("the range answers no entry with `end` 1, and the transaction goes on")]
fn no_entry(w: &mut Steps) -> TestResult {
    assert_eq!(w.kernel.batches.len(), 2, "{:?}", w.kernel.batches);
    for batch in &w.kernel.batches {
        assert!(batch.entries.is_empty(), "{batch:?}");
        assert_eq!(batch.end, EntriesEnd::Ended.number());
        assert!(batch.last);
    }
    w.roles = None;
    Ok(())
}

#[when("a transaction names in a Scan an IRI its home refuses, and the IRI is no skolem IRI")]
fn foreign_refused(w: &mut Steps) -> TestResult {
    one_process_of_two(w)?;
    let iri = homed_at_three(4)?;
    let script = HomeScript::refusing(ErrorCode::Refused.number());
    run(async {
        play_home(&script).await?;
        let mut client = dial_at("tcp://127.0.0.1:17200").await?;
        let (bind_stream, read) = bind_first(&mut client, 24).await?;
        let scan = term_scan(24, read, &iri);
        client.send_only(&scan).await?;
        let scan_stream = client.last_stream();
        let mut sequence = EntriesSequence::new(scan.ranges());
        for _ in 0..2 {
            if let Frame::Refused(refused) = next_frame(&mut client, &mut sequence).await? {
                w.kernel.refused.push(refused);
            }
        }
        w.kernel.numbers = vec![bind_stream, scan_stream];
        Ok(())
    })
}

#[then("the Scan's stream and the Bind stream each carry the home's refusal")]
fn refusal_on_both(w: &mut Steps) -> TestResult {
    let mut streams: Vec<u64> = w.kernel.refused.iter().map(|r| r.stream).collect();
    streams.sort_unstable();
    assert_eq!(streams, w.kernel.numbers, "{:?}", w.kernel.refused);
    for refused in &w.kernel.refused {
        assert_eq!(refused.code, ErrorCode::Refused, "{refused:?}");
    }
    w.roles = None;
    Ok(())
}

/// Commits a write of `<s> <p> <o>` as `request` of `session`: the Commit frame's body.
async fn committed_body(session: [u8; 16], request: u64, subject: &str) -> TestResult<Vec<u8>> {
    let mut client = dial().await?;
    drop(client.request(&bind_of(9, PRINCIPAL)).await?);
    let stream = client.last_stream();
    let write = edge_write(subject, "http://ex.org/o");
    write_held(&mut client, stream, &write).await?;
    client
        .send_on(stream, &Commit::new(session, request, 0))
        .await?;
    let answer = client.next_frame().await?;
    let view = answer.view()?;
    let _committed = CommitView::from_frame(&view)?;
    Ok(view.body().to_vec())
}

#[when("a session's request committed, and the frontend recalls it")]
fn recalled(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let session = [0xAB; 16];
    run(async {
        let body = committed_body(session, 1, "http://ex.org/r").await?;
        let mut client = dial().await?;
        let reply = client.request(&Recall::new(PARTITION, session, 1)).await?;
        let view = reply.view()?;
        assert_eq!(view.command(), Command::Commit);
        assert_ne!(view.header().flags() & FLAG_LAST, 0, "its stream ends");
        w.kernel.bytes = vec![body, view.body().to_vec()];
        Ok(())
    })
}

#[then("the partition answers with a Commit frame holding what the commit's reply held")]
fn recall_holds(w: &mut Steps) -> TestResult {
    let [committed, recalled] = w.kernel.bytes.as_slice() else {
        return Err(TestError::new("the two bodies"));
    };
    assert_eq!(committed, recalled, "byte for byte");
    w.roles = None;
    Ok(())
}

#[when(
    "the frontend recalls a request the table holds no reply for, then a request below the session's last"
)]
fn recalled_unheld(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let session = [0xAC; 16];
    let last = 5;
    run(async {
        let _body = committed_body(session, last, "http://ex.org/r").await?;
        let mut client = dial().await?;
        for request in [last + 1, last - 1] {
            client
                .send_only(&Recall::new(PARTITION, session, request))
                .await?;
            let mut sequence = EntriesSequence::new([]);
            if let Frame::Refused(refused) = next_frame(&mut client, &mut sequence).await? {
                w.kernel.refused.push(refused);
            }
        }
        w.kernel.numbers = vec![last];
        Ok(())
    })
}

#[then(
    "the first is refused with code 18 (NotCommitted), and the second with code 13 (RequestOutOfOrder), `detail` the last"
)]
fn recall_refused(w: &mut Steps) -> TestResult {
    let [unheld, below] = w.kernel.refused.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.kernel.refused)));
    };
    assert_eq!(unheld.code, ErrorCode::NotCommitted, "{unheld:?}");
    assert_eq!(below.code, ErrorCode::RequestOutOfOrder, "{below:?}");
    assert_eq!(Some(&below.detail), w.kernel.numbers.first());
    w.roles = None;
    Ok(())
}
