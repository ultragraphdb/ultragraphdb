/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a scenario does to its cluster: writes through real Binds and Commits, reads through a real ComputeRead, a Scene's tenant
//! its own.

use std::future::Future;
use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_ids::{Epoch, SystemId, Version};
use ultragraphdb_core_terms::{EncodedTerm, TermView, Text};
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BindView, BodyView, Closed, ClosedView, Command, Commit, CommitView,
    ComputeRead, Credit, ErrorCode, ErrorReplyView, FLAG_LAST, MAX_FRAME_FLOOR_U32,
    MAX_FRAME_SIZE_U32, PatternSlot, QuadPattern, ReadLevel, RequestTerm, ResultTermView,
    ResultView, Scan, ScanAt, ScanRange, TermTable, Write, WriteChange,
};
use ultragraphdb_store_quads::{iri_bucket, term_key};
use ultragraphdb_tests::{CLUSTER, SECRET};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::cluster::{ClusterInfo, fresh_tenant};

pub const PRINCIPAL: u64 = 1;
pub const KNOWS: &str = "http://ex.org/knows";
pub const NAME: &str = "http://ex.org/name";

/// How long a read's transaction has, in milliseconds, in a scenario with no deadline of its own.
const DEADLINE_MS: u32 = 2_000;

/// How long the test waits on one answer of a role: past every deadline and stall a role is given, so a role's own refusal
/// arrives first and an answer that never comes fails its step naming what it waited for.
pub const ANSWER_WAIT: Duration = Duration::from_secs(5);

/// How long one step's work may take in all.
const STEP_WAIT: Duration = Duration::from_secs(20);

/// Runs `work` on a runtime of its own, failing past [`STEP_WAIT`].
pub fn run<T>(work: impl Future<Output = TestResult<T>>) -> TestResult<T> {
    Runtime::new()?.block_on(within(STEP_WAIT, "the step's work", work))
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

/// The next frame of `client`, failing past [`ANSWER_WAIT`] naming `what` it waited for.
pub async fn answer_of(client: &mut RoleClient, what: &str) -> TestResult<ReceivedFrame> {
    within(ANSWER_WAIT, what, async { Ok(client.next_frame().await?) }).await
}

/// A term of a Result, as a test compares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    pub blank: bool,
}

/// One Result frame.
#[derive(Debug, Clone, Default)]
pub struct Batch {
    pub rows: Vec<Vec<Option<Cell>>>,
    pub resume: Vec<u8>,
    pub last: bool,
    pub size: usize,
    pub epoch: u64,
    pub partition: u32,
    pub columns: u32,
}

/// What a read answered.
#[derive(Debug, Clone, Default)]
pub struct ReadAnswer {
    pub batches: Vec<Batch>,
    pub refused: Option<Refused>,
}

impl ReadAnswer {
    /// Every row of every batch.
    pub fn rows(&self) -> Vec<Vec<Option<Cell>>> {
        self.batches
            .iter()
            .flat_map(|batch| batch.rows.iter().cloned())
            .collect()
    }

    /// Column `column` of every row, by its text; an empty cell's text is empty.
    pub fn column(&self, column: usize) -> Vec<String> {
        self.rows()
            .iter()
            .map(|row| {
                row.get(column)
                    .and_then(Clone::clone)
                    .map_or_else(String::new, |cell| cell.text)
            })
            .collect()
    }

    /// The last batch's resume key.
    pub fn resume(&self) -> Vec<u8> {
        self.batches
            .last()
            .map_or_else(Vec::new, |batch| batch.resume.clone())
    }

    /// The answer's refusal.
    pub fn refusal(&self) -> TestResult<&Refused> {
        self.refused.as_ref().context("a refusal")
    }

    /// Fails unless the read reached its `LAST` with no refusal.
    pub fn whole(&self) -> TestResult {
        if let Some(refused) = &self.refused {
            return Err(TestError::new(format!("the read was refused: {refused:?}")));
        }
        match self.batches.last() {
            Some(batch) if batch.last => Ok(()),
            other => Err(TestError::new(format!("no last batch: {other:?}"))),
        }
    }
}

/// An Error frame.
#[derive(Debug, Clone)]
pub struct Refused {
    pub code: ErrorCode,
    pub detail: u64,
    pub message: String,
}

/// The term's text: what it names by hash, or a literal's inline lexical form.
fn cell_of(term: ResultTermView<'_>) -> Cell {
    let blank = term.is_blank_node();
    let text = term.text().map(str::to_string).or_else(|| {
        let encoded = term.term()?;
        if let TermView::String(Text::Inline(inline)) = encoded.view().ok()? {
            Some(inline.to_string())
        } else {
            None
        }
    });
    Cell {
        text: text.unwrap_or_default(),
        blank,
    }
}

/// `frame`, a Result, as a test reads it.
fn batch_of(frame: &ReceivedFrame) -> TestResult<Batch> {
    let view = frame.view()?;
    let result = ResultView::from_frame(&view)?;
    result.check_pattern()?;
    let mut rows = Vec::with_capacity(result.rows() as usize);
    for row in 0..result.rows() {
        let mut cells = Vec::with_capacity(result.columns() as usize);
        for column in 0..result.columns() {
            let cell = result.cell(column, row).context("a cell")?;
            cells.push(result.term(cell).map(cell_of));
        }
        rows.push(cells);
    }
    Ok(Batch {
        rows,
        resume: result.resume().to_vec(),
        last: view.header().flags() & FLAG_LAST != 0,
        size: frame.bytes().len(),
        epoch: result.epoch(),
        partition: result.partition(),
        columns: result.columns(),
    })
}

/// The Error `frame` holds, if it is one.
pub fn refusal_of(frame: &ReceivedFrame) -> TestResult<Option<Refused>> {
    let view = frame.view()?;
    if view.command() != Command::Error {
        return Ok(None);
    }
    let error = ErrorReplyView::from_frame(&view)?;
    Ok(Some(Refused {
        code: error.code(),
        detail: error.detail(),
        message: error.message().to_string(),
    }))
}

/// A connection compute takes reads on, as the frontend's.
pub async fn compute_client(info: &ClusterInfo) -> TestResult<RoleClient> {
    let target = Address::parse(&info.compute_address())?;
    Ok(RoleClient::dial(&target, CLUSTER, "frontend", SECRET).await?)
}

/// Sends `read` on `client` and reads its answer, granting a frame's credit per batch.
pub async fn answer_on(client: &mut RoleClient, read: &ComputeRead) -> TestResult<ReadAnswer> {
    client.send_only(read).await?;
    let stream = client.last_stream();
    let mut answer = ReadAnswer::default();
    loop {
        let what = format!("batch {} of the read", answer.batches.len() + 1);
        let frame = answer_of(client, &what).await?;
        if let Some(refused) = refusal_of(&frame)? {
            answer.refused = Some(refused);
            return Ok(answer);
        }
        let batch = batch_of(&frame)?;
        drop(frame);
        let last = batch.last;
        answer.batches.push(batch);
        if last {
            return Ok(answer);
        }
        client.send_on(stream, &Credit::new(1)).await?;
    }
}

/// A batch read off `client`.
pub async fn next_batch(client: &mut RoleClient) -> TestResult<Batch> {
    let frame = answer_of(client, "a batch").await?;
    if let Some(refused) = refusal_of(&frame)? {
        return Err(TestError::new(format!("refused: {refused:?}")));
    }
    batch_of(&frame)
}

/// The next frame of `client`, a batch or a refusal.
pub async fn next_of(client: &mut RoleClient) -> TestResult<Result<Batch, Refused>> {
    let frame = answer_of(client, "a batch or a refusal").await?;
    match refusal_of(&frame)? {
        Some(refused) => Ok(Err(refused)),
        None => Ok(Ok(batch_of(&frame)?)),
    }
}

/// A quad pattern by its terms.
#[derive(Debug, Clone, Default)]
pub struct Pattern {
    pub graph: Option<GraphTerm>,
    pub subject: Option<String>,
    pub predicate: Option<String>,
    pub object: Option<RequestTerm>,
}

/// A pattern's bound graph.
#[derive(Debug, Clone)]
pub enum GraphTerm {
    Default,
    Iri(String),
}

/// An IRI as a pattern's object.
pub fn iri(text: &str) -> RequestTerm {
    RequestTerm::Iri(text.to_string())
}

/// An `xsd:string` literal.
pub fn string(text: &str) -> RequestTerm {
    RequestTerm::Literal {
        lexical: text.to_string(),
        datatype: None,
    }
}

impl Pattern {
    /// The pattern of the default graph.
    pub fn of(subject: Option<&str>, predicate: Option<&str>, object: Option<RequestTerm>) -> Self {
        Self {
            graph: Some(GraphTerm::Default),
            subject: subject.map(str::to_string),
            predicate: predicate.map(str::to_string),
            object,
        }
    }

    /// The pattern of the graph `iri`.
    #[must_use]
    pub fn in_graph(mut self, iri: &str) -> Self {
        self.graph = Some(GraphTerm::Iri(iri.to_string()));
        self
    }

    /// The pattern with its graph unbound.
    #[must_use]
    pub fn any_graph(mut self) -> Self {
        self.graph = None;
        self
    }

    /// The pattern's slots over a new term table.
    fn table(&self) -> TestResult<(TermTable, QuadPattern)> {
        let mut terms = TermTable::new();
        let graph = match &self.graph {
            None => PatternSlot::Unbound,
            Some(GraphTerm::Default) => PatternSlot::DefaultGraph,
            Some(GraphTerm::Iri(text)) => PatternSlot::Bound(terms.iri(text)),
        };
        let mut bound = |text: &Option<String>| {
            text.as_ref().map_or(PatternSlot::Unbound, |text| {
                PatternSlot::Bound(terms.iri(text))
            })
        };
        let subject = bound(&self.subject);
        let predicate = bound(&self.predicate);
        let object = match &self.object {
            Some(term) => PatternSlot::Bound(terms.intern(term.clone())?),
            None => PatternSlot::Unbound,
        };
        Ok((terms, QuadPattern::new(graph, subject, predicate, object)))
    }
}

/// One scenario's place in a cluster: a tenant nothing else wrote.
#[derive(Debug, Clone, Copy)]
pub struct Scene {
    pub info: &'static ClusterInfo,
    pub tenant: u64,
}

impl Scene {
    /// A scene of a tenant of its own, with what the taps saw forgotten.
    pub fn new(info: &'static ClusterInfo) -> Self {
        info.log.clear();
        Self {
            info,
            tenant: fresh_tenant(),
        }
    }

    /// The partition owning the bucket of `iri`.
    pub fn owner(&self, iri: &str) -> TestResult<u32> {
        let map = PlacementMap::even(self.info.partitions, NodeId::new(1))?;
        let bucket = iri_bucket(self.tenant, iri).context("a bucket")?;
        Ok(map.partition_of(bucket, Epoch::ZERO).get())
    }

    /// An IRI named after `name` that `partition` homes.
    pub fn iri_in(&self, partition: u32, name: &str) -> TestResult<String> {
        for at in 0..100_000 {
            let iri = format!("http://ex.org/{name}{at}");
            if self.owner(&iri)? == partition {
                return Ok(iri);
            }
        }
        Err(TestError::new("no IRI of the partition"))
    }

    /// A client of `partition`, as the frontend dials it.
    pub async fn partition_client(&self, partition: u32) -> TestResult<RoleClient> {
        let target = Address::parse(&format!(
            "tcp://127.0.0.1:{}",
            self.info.partition_port(partition)
        ))?;
        Ok(RoleClient::dial(&target, CLUSTER, "frontend", SECRET).await?)
    }

    /// Commits `write` at `partition` through no session: its commit version.
    pub async fn commit_version(&self, partition: u32, write: &Write) -> TestResult<u64> {
        let mut transaction = self.begin(partition, 7).await?;
        transaction.write(write).await?;
        transaction.commit().await
    }

    /// Commits `write` at `partition` through no session.
    pub async fn commit(&self, partition: u32, write: &Write) -> TestResult {
        self.commit_version(partition, write).await.map(drop)
    }

    /// Commits `subject` `predicate` `object`, at the subject's partition.
    pub async fn insert(&self, subject: &str, predicate: &str, object: &RequestTerm) -> TestResult {
        self.insert_version(subject, predicate, object)
            .await
            .map(drop)
    }

    /// Commits `subject` `predicate` `object`, at the subject's partition: its commit version.
    pub async fn insert_version(
        &self,
        subject: &str,
        predicate: &str,
        object: &RequestTerm,
    ) -> TestResult<u64> {
        let partition = self.owner(subject)?;
        self.commit_version(partition, &insert_of(None, subject, predicate, object)?)
            .await
    }

    /// A read-write transaction bound at `partition`, named `identity`.
    pub async fn begin(&self, partition: u32, identity: u8) -> TestResult<Transaction> {
        let mut client = self.partition_client(partition).await?;
        let bind = Bind::new(
            partition,
            BindKind::Transaction,
            self.tenant,
            0,
            PRINCIPAL,
            [identity; 16],
        );
        let reply = client.request(&bind).await?;
        let read_version = BindView::from_frame(&reply.view()?)?.read_version();
        drop(reply);
        let stream = client.last_stream();
        Ok(Transaction {
            client,
            stream,
            partition,
            read_version,
            identity: [identity; 16],
        })
    }

    /// Waits until every partition but those held has closed `epoch`.
    pub async fn closed_through(&self, epoch: u64, partitions: &[u32]) -> TestResult {
        for partition in partitions {
            let mut client = self.partition_client(*partition).await?;
            let what = format!("partition {partition}'s close of epoch {epoch}");
            within(ANSWER_WAIT, &what, async {
                while closed_at(&mut client, *partition).await? < epoch {
                    compio::time::sleep(Duration::from_millis(1)).await;
                }
                Ok(())
            })
            .await?;
        }
        Ok(())
    }

    /// A snapshot epoch at or above everything committed so far, about to close.
    pub fn snapshot_epoch(&self) -> u64 {
        self.info.clock.latest() + 1
    }

    /// `pattern` read at snapshot level at `epoch`.
    pub fn snapshot_at(&self, pattern: &Pattern, epoch: u64) -> TestResult<ComputeRead> {
        let bound = Version::bound_of(Epoch::new(epoch)?);
        Ok(self
            .read_of(pattern, true, ReadLevel::Snapshot)?
            .with_read_version(bound)
            .with_epoch(epoch))
    }

    /// `pattern` read at snapshot level past everything committed so far.
    pub fn snapshot(&self, pattern: &Pattern) -> TestResult<ComputeRead> {
        self.snapshot_at(pattern, self.snapshot_epoch())
    }

    /// `pattern` read at `level`, the client's frame and credit generous, with nothing else set.
    pub fn read_of(
        &self,
        pattern: &Pattern,
        read_only: bool,
        level: ReadLevel,
    ) -> TestResult<ComputeRead> {
        let (terms, slots) = pattern.table()?;
        Ok(
            ComputeRead::new(self.tenant, read_only, level, terms, slots, Vec::new())
                .with_principal(PRINCIPAL)
                .with_deadline_ms(DEADLINE_MS)
                .with_client_limits(MAX_FRAME_SIZE_U32, 4),
        )
    }

    /// `read`'s answer, on a connection of its own.
    pub async fn answer(&self, read: &ComputeRead) -> TestResult<ReadAnswer> {
        let mut client = compute_client(self.info).await?;
        answer_on(&mut client, read).await
    }

    /// The ID the dictionary of `partition` holds for `iri`.
    pub async fn id_in(&self, partition: u32, iri: &str) -> TestResult<SystemId> {
        let mut client = self.partition_client(partition).await?;
        let key = term_key(self.tenant, iri);
        let past = [key.as_slice(), &[0]].concat();
        let mut scan = Scan::new(ScanAt::Latest, 0, 0, PRINCIPAL, [0; 16], 0, 0);
        scan.push_range(ScanRange::new(partition, &key, &past));
        client.send_only(&scan).await?;
        let frame = client.next_frame().await?;
        let view = frame.view()?;
        let entries = ultragraphdb_protocol_wire::EntriesView::from_frame(&view)?;
        let value = entries
            .entries()
            .iter()
            .next()
            .and_then(|entry| entry.value().map(<[u8]>::to_vec))
            .context("the term's entry")?;
        Ok(SystemId::decode(&value)?)
    }

    /// `iris` in the store's order of their IDs, each homed at the partition holding it.
    pub async fn in_key_order(&self, iris: &[String]) -> TestResult<Vec<String>> {
        let mut keyed = Vec::with_capacity(iris.len());
        for iri in iris {
            let id = self.id_in(self.owner(iri)?, iri).await?;
            keyed.push((EncodedTerm::id(id).as_bytes().to_vec(), iri.clone()));
        }
        keyed.sort();
        Ok(keyed.into_iter().map(|(_, iri)| iri).collect())
    }
}

/// A partition's latest closed epoch.
pub async fn closed_at(client: &mut RoleClient, partition: u32) -> TestResult<u64> {
    let reply = client.request(&Closed::request(&[partition])).await?;
    let view = reply.view()?;
    let closed = ClosedView::from_frame(&view)?;
    Ok(closed.partition(0).context("the partition")?.epoch())
}

/// A Write inserting `subject` `predicate` `object` in `graph`, the default graph for `None`.
pub fn insert_of(
    graph: Option<&str>,
    subject: &str,
    predicate: &str,
    object: &RequestTerm,
) -> TestResult<Write> {
    let mut table = TermTable::new();
    let graph = graph.map(|iri| table.iri(iri));
    let subject = table.iri(subject);
    let predicate = table.iri(predicate);
    let object = table.intern(object.clone())?;
    Ok(Write::new(
        table,
        vec![WriteChange::Insert {
            graph,
            subject,
            predicate,
            object,
        }],
    ))
}

/// A read-write transaction, bound through a played frontend's connection.
#[derive(Debug)]
pub struct Transaction {
    pub client: RoleClient,
    pub stream: u64,
    pub partition: u32,
    pub read_version: u64,
    pub identity: [u8; 16],
}

impl Transaction {
    /// Sends `write` and waits for its Held.
    pub async fn write(&mut self, write: &Write) -> TestResult {
        self.client.send_on(self.stream, write).await?;
        let what = format!("the Held of a Write at partition {}", self.partition);
        let held = answer_of(&mut self.client, &what).await?;
        if let Some(refused) = refusal_of(&held)? {
            return Err(TestError::new(format!(
                "the write was refused: {refused:?}"
            )));
        }
        Ok(())
    }

    /// Commits: the commit version.
    pub async fn commit(&mut self) -> TestResult<u64> {
        match self.try_commit().await? {
            Ok(version) => Ok(version),
            Err(refused) => Err(TestError::new(format!(
                "the commit was refused: {refused:?}"
            ))),
        }
    }

    /// Commits: the commit version, or its refusal.
    pub async fn try_commit(&mut self) -> TestResult<Result<u64, Refused>> {
        self.client
            .send_on(self.stream, &Commit::new([0; 16], 0, 0))
            .await?;
        let what = format!(
            "the answer to a Commit at partition {} over version {}",
            self.partition, self.read_version
        );
        let answer = answer_of(&mut self.client, &what).await?;
        if let Some(refused) = refusal_of(&answer)? {
            return Ok(Err(refused));
        }
        Ok(Ok(CommitView::from_frame(&answer.view()?)?.commit_version()))
    }

    /// `pattern` read in the transaction, of `limit` rows, 0 for no limit.
    pub fn read(&self, scene: &Scene, pattern: &Pattern, limit: u32) -> TestResult<ComputeRead> {
        Ok(scene
            .read_of(pattern, false, ReadLevel::Snapshot)?
            .with_partition(self.partition)
            .with_transaction(self.identity)
            .with_read_version(self.read_version)
            .with_limit(limit))
    }
}

/// The largest client frame the floor allows.
pub const FLOOR: u32 = MAX_FRAME_FLOOR_U32;
