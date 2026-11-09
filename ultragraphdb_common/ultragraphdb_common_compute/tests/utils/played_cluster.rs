/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A cluster played in the test's own process for compute's reads: a frontend answering Placement and Members, partitions
//! answering each Scan from a table of keys and recording what they were asked, and a compute core serving its sessions, all on
//! the test's runtime over loopback TCP. What compute sends and answers crosses real role connections, built by the wire crate.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::Future;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::spawn;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap, TenantRun};
use ultragraphdb_common_compute::{ComputeCore, ComputeSession, ReadTimes};
use ultragraphdb_common_role::{
    Answer, COMPUTE_WELCOME_CREDIT, OpenRequests, PARTITION_INITIAL_CREDIT, Push, ROLE_ITERATIONS,
    RoleClient, RoleCluster, RoleCredentials, RoleSecret, Server, Session, serve_connection,
};
use ultragraphdb_core_ids::{Bucket, Epoch, Sequence, SystemId, Tenant, TypePrefix, Version};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, BucketRange, Closed, ClosedPartition, Command, Entries, EntriesEnd,
    ErrorCode, ErrorReply, FLAG_LAST, Header, MAX_FRAME_SIZE, Members, ObjectRecordsBuilder,
    Placement, PreferredLeader, ScanAt, ScanView,
};
use ultragraphdb_store_quads::{id_key, iri_bucket, term_key};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// The played cluster's identity.
pub const CLUSTER: u64 = 7;

/// The role secret every played role holds.
const PASSWORD: &str = "played-role-secret";

/// The tenant every played read reads.
pub const TENANT: u64 = 1;

/// The epoch every played partition has closed and answers at.
pub const EPOCH: u64 = 9;

/// How long a test's whole work may take.
pub const WAIT: Duration = Duration::from_secs(20);

/// Secret files, one per played cluster, since tests run side by side.
static FILES: AtomicU32 = AtomicU32::new(0);

/// The version bound of [`EPOCH`], the version a played partition answers a read that names none at.
pub fn version() -> TestResult<u64> {
    Ok(Version::bound_of(Epoch::new(EPOCH)?))
}

/// The system ID of `sequence` in [`TENANT`]'s run.
pub fn id(sequence: u64) -> TestResult<SystemId> {
    let run = TenantRun::of(Tenant::new(TENANT)).range();
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(run.start())?,
        Sequence::new(sequence)?,
    ))
}

/// A list's value: one object record of each of `objects`, by its encoding, with no statement.
pub fn list_of(objects: &[EncodedTerm]) -> Vec<u8> {
    let mut records = ObjectRecordsBuilder::new();
    for object in objects {
        records.push(object.as_bytes(), &[]);
    }
    records.value().to_vec()
}

/// `work`'s outcome, or a failure naming `what` once `bound` has passed without one.
pub async fn within<T>(
    bound: Duration,
    what: &str,
    work: impl Future<Output = TestResult<T>>,
) -> TestResult<T> {
    compio::time::timeout(bound, work)
        .await
        .map_err(|_| TestError::new(format!("{what} did not come within {bound:?}")))?
}

/// Runs `work` on a runtime of its own, failing past [`WAIT`].
pub fn run<T>(work: impl Future<Output = TestResult<T>>) -> TestResult<T> {
    compio::runtime::Runtime::new()?.block_on(within(WAIT, "the test's work", work))
}

/// The predicate the rows of [`hold_rows`] name, by the ID of sequence 1.
pub const ROWS_PREDICATE: &str = "http://ex.org/p";

/// `partition` holding `<subject> <p> <object>` in the default graph for each of `rows`, the IRIs by the IDs of sequences 10, 11,
/// 12, … in order; the IDs of each row's subject and object.
pub fn hold_rows(
    partition: &Partition,
    rows: &[(String, String)],
) -> TestResult<Vec<(SystemId, SystemId)>> {
    let predicate = id(1)?;
    partition.term(ROWS_PREDICATE, predicate);
    let mut ids = Vec::with_capacity(rows.len());
    for (at, (subject, object)) in (10u64..).step_by(2).zip(rows) {
        let (s, o) = (id(at)?, id(at + 1)?);
        partition.term(subject, s);
        partition.term(object, o);
        partition.quads(Graph::Default, s, predicate, &[EncodedTerm::id(o)]);
        ids.push((s, o));
    }
    Ok(ids)
}

/// `n` rows `<http://ex.org/s{i}> <p> <http://ex.org/o{i}>`.
pub fn short_rows(n: usize) -> Vec<(String, String)> {
    (0..n)
        .map(|i| (format!("http://ex.org/s{i}"), format!("http://ex.org/o{i}")))
        .collect()
}

/// Each row's cells in a read of `?s <p> ?o`: the subject, the object, and no reifier.
pub fn cells_of(rows: &[(String, String)]) -> Vec<Vec<Option<String>>> {
    rows.iter()
        .map(|(s, o)| vec![Some(s.clone()), Some(o.clone()), None])
        .collect()
}

/// One range a played partition was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskedRange {
    pub first: Vec<u8>,
    pub past: Vec<u8>,
    pub after: Vec<u8>,
}

/// One Scan a played partition was asked, on its connection's `connection`th stream `stream`.
#[derive(Debug, Clone)]
pub struct Asked {
    pub connection: u32,
    pub stream: u64,
    pub at: ScanAt,
    pub read_version: u64,
    pub epoch: u64,
    pub limit: u32,
    pub ranges: Vec<AskedRange>,
}

impl Asked {
    /// Whether the Scan holds a version and reads nothing: several ranges, each of one key.
    pub fn is_pin(&self) -> bool {
        self.ranges.len() > 1
            && self
                .ranges
                .first()
                .is_some_and(|first| self.ranges.iter().all(|range| *range == *first))
    }
}

/// A partition played by the test: its table, and what it was asked.
#[derive(Debug, Default)]
pub struct Partition {
    number: u32,
    table: RefCell<BTreeMap<Vec<u8>, Vec<u8>>>,
    scans: RefCell<Vec<Asked>>,
    credits: RefCell<Vec<(u32, u64, u32)>>,
    cancels: RefCell<Vec<(u32, u64)>>,
    connections: Cell<u32>,
    split: Cell<bool>,
}

impl Partition {
    /// Holds `value` under `key`.
    pub fn put(&self, key: Vec<u8>, value: Vec<u8>) {
        let _old = self.table.borrow_mut().insert(key, value);
    }

    /// Holds `iri` under `id`, by its text and by its ID.
    pub fn term(&self, iri: &str, id: SystemId) {
        self.put(term_key(TENANT, iri), id.to_be_bytes().to_vec());
        self.put(id_key(TENANT, id), iri.as_bytes().to_vec());
    }

    /// Holds `subject` `predicate` each of `objects` in `graph`, as the forward list and each object's reverse list hold it.
    pub fn quads(
        &self,
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
        objects: &[EncodedTerm],
    ) {
        let forward = QuadKey::Forward {
            tenant: TENANT,
            graph,
            predicate,
            subject,
        };
        self.put(forward.to_bytes(), list_of(objects));
        for object in objects {
            let reverse = QuadKey::Reverse {
                tenant: TENANT,
                graph,
                predicate,
                object: object.clone(),
            };
            let key = reverse.to_bytes();
            let mut subjects: Vec<EncodedTerm> = self
                .table
                .borrow()
                .get(&key)
                .and_then(|value| ultragraphdb_protocol_wire::ObjectRecords::read(value).ok())
                .map(|records| {
                    records
                        .iter()
                        .filter_map(|record| EncodedTerm::from_bytes(record.encoding()).ok())
                        .collect()
                })
                .unwrap_or_default();
            subjects.push(EncodedTerm::id(subject));
            subjects.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            self.put(key, list_of(&subjects));
        }
    }

    /// Holds the quad `<s> <p> <o>` of the default graph, each IRI by the ID of its sequence in `ids`.
    pub fn quad(&self, (s, p, o): (&str, &str, &str), ids: (u64, u64, u64)) -> TestResult {
        let (subject, predicate, object) = (id(ids.0)?, id(ids.1)?, id(ids.2)?);
        self.term(s, subject);
        self.term(p, predicate);
        self.term(o, object);
        self.quads(
            Graph::Default,
            subject,
            predicate,
            &[EncodedTerm::id(object)],
        );
        Ok(())
    }

    /// Answers each entry in a frame of its own from here on, its range's end on the last.
    pub fn split_frames(&self) {
        self.split.set(true);
    }

    /// Every Scan it was asked, in order.
    pub fn scans(&self) -> Vec<Asked> {
        self.scans.borrow().clone()
    }

    /// The Scans holding a version.
    pub fn pins(&self) -> Vec<Asked> {
        self.scans().into_iter().filter(Asked::is_pin).collect()
    }

    /// The frames of credit `stream` of `connection` was granted.
    pub fn credited(&self, connection: u32, stream: u64) -> u32 {
        self.credits
            .borrow()
            .iter()
            .filter(|(held, on, _)| *held == connection && *on == stream)
            .map(|(_, _, frames)| *frames)
            .sum()
    }

    /// Whether `stream` of `connection` was cancelled.
    pub fn cancelled(&self, connection: u32, stream: u64) -> bool {
        self.cancels.borrow().contains(&(connection, stream))
    }

    /// The entries `range` reads, at most `limit` of them, and whether the limit stopped it.
    fn entries_in(&self, range: &AskedRange, limit: u32) -> Found {
        let past = if range.past.is_empty() {
            end_of_tenant(&range.first)
        } else {
            range.past.clone()
        };
        let table = self.table.borrow();
        let mut found: Vec<(Vec<u8>, Vec<u8>)> = table
            .range(range.first.clone()..)
            .take_while(|(key, _)| past.is_empty() || **key < past)
            .map(|(key, value)| {
                let resumed = !range.after.is_empty() && *key == range.first;
                let value = if resumed {
                    records_from(value, &range.after)
                } else {
                    value.clone()
                };
                (key.clone(), value)
            })
            .collect();
        let cap = if limit == 0 {
            usize::MAX
        } else {
            limit as usize
        };
        let limited = found.len() >= cap && limit != 0;
        found.truncate(cap);
        (found, limited)
    }
}

/// A range's entries, and whether the Scan's limit stopped it.
type Found = (Vec<(Vec<u8>, Vec<u8>)>, bool);

/// The key past the last of the tenant and keyspace `first` names in its first nine bytes; empty past the last such key.
fn end_of_tenant(first: &[u8]) -> Vec<u8> {
    let mut end: Vec<u8> = first.iter().take(9).copied().collect();
    while let Some(last) = end.pop() {
        if last < u8::MAX {
            end.push(last + 1);
            return end;
        }
    }
    end
}

/// The object records of `value` from the object `after` on, that object included.
fn records_from(value: &[u8], after: &[u8]) -> Vec<u8> {
    let mut kept = ObjectRecordsBuilder::new();
    if let Ok(records) = ultragraphdb_protocol_wire::ObjectRecords::read(value) {
        for record in records
            .iter()
            .skip_while(|record| record.encoding() != after)
        {
            let statements: Vec<[u8; 16]> = record.statement_ids().collect();
            kept.push(record.encoding(), &statements);
        }
    }
    kept.value().to_vec()
}

/// A frame of `body` on `stream`, flagged last when `last`.
fn frame_of<B: BodyBuilder>(body: &B, stream: u64, last: bool) -> Result<Vec<u8>, ErrorReply> {
    let header = Header::new(CLUSTER, B::COMMAND).with_stream(stream);
    let header = if last {
        header.with_flags(FLAG_LAST)
    } else {
        header
    };
    body.frame(header)
        .map_err(|rule| ErrorReply::protocol(&rule))
}

/// One connection at a played partition.
#[derive(Debug)]
struct PartitionConnection {
    partition: Rc<Partition>,
    connection: u32,
    pushes: UnboundedSender<(u64, Vec<u8>)>,
    pushed: UnboundedReceiver<(u64, Vec<u8>)>,
}

impl PartitionConnection {
    /// Queues the frames answering `asked`: each range's entries, one frame or as many as fit, the Scan's last flagged.
    fn answer_scan(&self, asked: &Asked) -> Result<(), ErrorReply> {
        let version = match asked.at {
            ScanAt::ReadVersion | ScanAt::Named => asked.read_version,
            ScanAt::Latest | ScanAt::LatestClosed => version()
                .map_err(|error| ErrorReply::new(ErrorCode::Unavailable, error.to_string()))?,
        };
        let count = asked.ranges.len();
        for (index, range) in asked.ranges.iter().enumerate() {
            let (entries, limited) = self.partition.entries_in(range, asked.limit);
            let end = if limited {
                EntriesEnd::Limit
            } else {
                EntriesEnd::Ended
            };
            let number = u32::try_from(index).unwrap_or(u32::MAX);
            let frames = self.frames_of(number, &entries, version);
            let held = frames.len();
            for (at, frame) in frames.into_iter().enumerate() {
                let ends = at + 1 == held;
                let frame = if ends { frame.with_end(end) } else { frame };
                let built = frame_of(&frame, asked.stream, ends && index + 1 == count)?;
                drop(self.pushes.unbounded_send((asked.stream, built)));
            }
        }
        Ok(())
    }

    /// The Entries frames of one range's `entries`, each to be continued until its end is set.
    fn frames_of(&self, range: u32, entries: &[(Vec<u8>, Vec<u8>)], version: u64) -> Vec<Entries> {
        let fresh = || Entries::new(range, EntriesEnd::More, EPOCH).with_version(version);
        let mut frames = vec![fresh()];
        for (key, value) in entries {
            let full = frames.last().is_some_and(|frame| {
                !frame.entries().is_empty()
                    && (self.partition.split.get()
                        || frame.frame_size_with(key.len(), value.len()) > MAX_FRAME_SIZE)
            });
            if full {
                frames.push(fresh());
            }
            if let Some(frame) = frames.last_mut() {
                frame.push(key, value);
            }
        }
        frames
    }

    /// The Scan `view` asks, recorded.
    fn asked(&self, stream: u64, view: &ScanView<'_>) -> Asked {
        let asked = Asked {
            connection: self.connection,
            stream,
            at: view.at(),
            read_version: view.read_version(),
            epoch: view.epoch(),
            limit: view.limit(),
            ranges: view
                .ranges()
                .map(|range| AskedRange {
                    first: range.first().to_vec(),
                    past: range.past().to_vec(),
                    after: range.after().to_vec(),
                })
                .collect(),
        };
        self.partition.scans.borrow_mut().push(asked.clone());
        asked
    }
}

impl Session for PartitionConnection {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let view = match frame.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        if view.command() == Command::Closed {
            let reply = Closed::new(vec![ClosedPartition::new(self.partition.number, EPOCH)]);
            return match frame_of(&reply, stream, true) {
                Ok(replied) => Answer::Reply(replied),
                Err(refusal) => Answer::Refuse(refusal),
            };
        }
        let scan = match ScanView::from_frame(&view) {
            Ok(scan) => scan,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let asked = self.asked(stream, &scan);
        match self.answer_scan(&asked) {
            Ok(()) => Answer::Open,
            Err(refusal) => Answer::Refuse(refusal),
        }
    }

    async fn next_push(&mut self) -> (u64, Push) {
        match self.pushed.next().await {
            Some((stream, frame)) => (stream, Push::Frame(frame)),
            // The connection holds a sender, so its channel never ends while it waits here.
            None => std::future::pending().await,
        }
    }

    fn cancelled(&mut self, stream: u64) {
        self.partition
            .cancels
            .borrow_mut()
            .push((self.connection, stream));
    }

    fn credited(&mut self, stream: u64, frames: u32) {
        self.partition
            .credits
            .borrow_mut()
            .push((self.connection, stream, frames));
    }
}

/// One connection at the played frontend: the even placement over `partitions`, and where each listens.
#[derive(Debug, Clone)]
struct FrontendConnection {
    partitions: u32,
    listed: Rc<RefCell<Vec<(u32, String)>>>,
}

impl FrontendConnection {
    /// The even placement over the partitions, as a frontend's first start forms it.
    fn placement(&self) -> TestResult<Placement> {
        let map = PlacementMap::even(self.partitions, NodeId::new(1))?;
        let mut reply = Placement::reply(0, 0, 1, 0);
        for (range, partition) in map.ranges_at(Epoch::ZERO) {
            reply.push_range(BucketRange::new(
                range.start(),
                range.end(),
                partition.get(),
            ));
        }
        for partition in 1..=self.partitions {
            reply.push_leader(PreferredLeader::new(partition, 1));
        }
        Ok(reply)
    }
}

impl Session for FrontendConnection {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let command = frame.view().map_or(Command::Error, |view| view.command());
        let built = if command == Command::Members {
            let mut members = Members::new();
            for (partition, address) in self.listed.borrow().iter() {
                members.push_storage(*partition, address.clone());
            }
            frame_of(&members, stream, true)
        } else if command == Command::Placement {
            self.placement()
                .map_err(|error| ErrorReply::new(ErrorCode::Unavailable, error.to_string()))
                .and_then(|reply| frame_of(&reply, stream, true))
        } else {
            Err(ErrorReply::new(ErrorCode::Unavailable, "not played"))
        };
        built.map_or_else(Answer::Refuse, Answer::Reply)
    }
}

/// The played cluster: its partitions, the compute core's address, and what compute's sessions hold open.
#[derive(Debug)]
pub struct Played {
    partitions: Vec<Rc<Partition>>,
    compute: Address,
    map: PlacementMap,
    open: OpenRequests,
}

/// The role credentials of [`PASSWORD`], from a file of their own.
fn secret() -> TestResult<RoleSecret> {
    let file = std::env::temp_dir().join(format!(
        "compute-played-secret-{}-{}",
        std::process::id(),
        FILES.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&file, PASSWORD)?;
    Ok(RoleSecret::read(&file.display().to_string())?)
}

/// Serves every connection a loopback port of its own takes as `server`, each by the session `session` makes.
async fn serve<S: Session + 'static>(
    server: Server,
    credentials: RoleCredentials,
    session: impl Fn() -> S + 'static,
) -> TestResult<Address> {
    let listener = Listener::Tcp(compio::net::TcpListener::bind("127.0.0.1:0").await?);
    let address = listener.local_address()?;
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (server, credentials, session) = (server.clone(), credentials.clone(), session());
            spawn(async move { serve_connection(carrier, &server, credentials, session).await })
                .detach();
        }
    })
    .detach();
    Ok(address)
}

impl Played {
    /// A cluster of `partitions` partitions and a compute core reading by `times`, served on this runtime.
    pub async fn start(partitions: u32, times: ReadTimes) -> TestResult<Self> {
        let secret = secret()?;
        let credentials = secret.credentials()?;
        let listed: Rc<RefCell<Vec<(u32, String)>>> = Rc::default();
        let mut played = Vec::new();
        for number in 1..=partitions {
            let partition = Rc::new(Partition {
                number,
                ..Partition::default()
            });
            let server = Server::new(CLUSTER, "storage", Endpoint::Partition)?
                .with_welcome(|welcome| welcome.with_initial_credit(PARTITION_INITIAL_CREDIT))
                .with_iterations(ROLE_ITERATIONS);
            let held = Rc::clone(&partition);
            let address = serve(server, credentials.clone(), move || {
                let (pushes, pushed) = unbounded();
                let connection = held.connections.get().saturating_add(1);
                held.connections.set(connection);
                PartitionConnection {
                    partition: Rc::clone(&held),
                    connection,
                    pushes,
                    pushed,
                }
            })
            .await?;
            listed.borrow_mut().push((number, address.to_string()));
            played.push(partition);
        }
        let frontend_server =
            Server::new(CLUSTER, "frontend", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
        let connection = FrontendConnection { partitions, listed };
        let frontend = serve(frontend_server, credentials.clone(), move || {
            connection.clone()
        })
        .await?;
        let cluster = RoleCluster::new(CLUSTER, secret, Duration::from_secs(5));
        let core = ComputeCore::new(cluster, frontend, times, "compute");
        let open = OpenRequests::default();
        let compute_server = Server::new(CLUSTER, "role", Endpoint::Compute)?
            .with_welcome(|welcome| welcome.with_initial_credit(COMPUTE_WELCOME_CREDIT))
            .with_iterations(ROLE_ITERATIONS);
        let held = open.clone();
        let compute = serve(compute_server, credentials, move || {
            ComputeSession::new(Rc::clone(&core)).with_open(held.clone())
        })
        .await?;
        Ok(Self {
            partitions: played,
            compute,
            map: PlacementMap::even(partitions, NodeId::new(1))?,
            open,
        })
    }

    /// Partition `number`.
    pub fn partition(&self, number: u32) -> TestResult<&Rc<Partition>> {
        self.partitions
            .get((number as usize).saturating_sub(1))
            .context("a played partition")
    }

    /// The partition that owns `iri`'s bucket, as the placement places it.
    pub fn owner(&self, iri: &str) -> TestResult<u32> {
        let bucket = iri_bucket(TENANT, iri).context("an IRI's bucket")?;
        Ok(self.map.partition_of(bucket, Epoch::ZERO).get())
    }

    /// The first IRI `http://ex.org/{stem}{n}` whose bucket `partition` owns.
    pub fn owned_by(&self, partition: u32, stem: &str) -> TestResult<String> {
        (0..1_000)
            .map(|n| format!("http://ex.org/{stem}{n}"))
            .find(|iri| self.owner(iri).is_ok_and(|owner| owner == partition))
            .context("an IRI the partition owns")
    }

    /// The placement the cluster's frontend answers.
    pub fn map(&self) -> &PlacementMap {
        &self.map
    }

    /// How many of compute's reads are open.
    pub fn open_reads(&self) -> usize {
        self.open.count()
    }

    /// A connection to compute, as the frontend's.
    pub async fn client(&self) -> TestResult<RoleClient> {
        Ok(RoleClient::dial(&self.compute, CLUSTER, "frontend", PASSWORD).await?)
    }
}
