/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The frontend's side of a played read: a ComputeRead built from a pattern's terms, and compute's answer as a test reads it.

use std::time::Duration;

use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_terms::{TermView, Text};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ComputeRead, ComputeReadView, Credit, ErrorCode,
    ErrorReplyView, FLAG_LAST, FrameView, Header, MAX_FRAME_FLOOR, MAX_FRAME_SIZE, PatternSlot,
    QuadPattern, ReadLevel, RequestTerm, ResultTermView, ResultView, TermTable,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::played_cluster::{CLUSTER, EPOCH, TENANT, version, within};

/// The principal every played read is a client's of.
pub const PRINCIPAL: u64 = 3;

/// The milliseconds a played read's transaction has left.
const DEADLINE_MS: u32 = 5_000;

/// How long one frame of compute's may take.
const FRAME_WAIT: Duration = Duration::from_secs(10);

/// A pattern's graph slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphTerm {
    Default,
    Unbound,
}

/// A quad pattern by its terms.
#[derive(Debug, Clone)]
pub struct Pattern {
    pub graph: GraphTerm,
    pub subject: Option<String>,
    pub predicate: Option<String>,
    pub object: Option<RequestTerm>,
}

impl Pattern {
    /// The pattern of the default graph.
    pub fn of(subject: Option<&str>, predicate: Option<&str>, object: Option<RequestTerm>) -> Self {
        Self {
            graph: GraphTerm::Default,
            subject: subject.map(str::to_string),
            predicate: predicate.map(str::to_string),
            object,
        }
    }

    /// The pattern with its graph `graph`.
    #[must_use]
    pub fn in_graph(mut self, graph: GraphTerm) -> Self {
        self.graph = graph;
        self
    }

    /// The pattern's slots over a new term table.
    fn table(&self) -> TestResult<(TermTable, QuadPattern)> {
        let mut terms = TermTable::new();
        let graph = match &self.graph {
            GraphTerm::Unbound => PatternSlot::Unbound,
            GraphTerm::Default => PatternSlot::DefaultGraph,
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

/// An IRI as a term.
pub fn iri(text: &str) -> RequestTerm {
    RequestTerm::Iri(text.to_string())
}

/// `pattern` read at `level`, from `resume`, the client's frame the smallest any client takes and its credit 4.
pub fn read_of(
    pattern: &Pattern,
    read_only: bool,
    level: ReadLevel,
    resume: Vec<u8>,
) -> TestResult<ComputeRead> {
    let (terms, slots) = pattern.table()?;
    Ok(
        ComputeRead::new(TENANT, read_only, level, terms, slots, resume)
            .with_principal(PRINCIPAL)
            .with_deadline_ms(DEADLINE_MS)
            .with_client_limits(MAX_FRAME_FLOOR as u32, 4),
    )
}

/// `pattern` read at snapshot level at [`EPOCH`], at its version bound.
pub fn snapshot(pattern: &Pattern) -> TestResult<ComputeRead> {
    Ok(read_of(pattern, true, ReadLevel::Snapshot, Vec::new())?
        .with_read_version(version()?)
        .with_epoch(EPOCH))
}

/// The pattern hash compute keys `read`'s resume key by: its pattern's sections, as the frame carries them.
pub fn pattern_hash(read: &ComputeRead) -> TestResult<u64> {
    let frame = read.frame(Header::new(CLUSTER, Command::ComputeRead))?;
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    Ok(ComputeReadView::from_frame(&view)?.pattern_hash())
}

/// One Result frame, each cell by its text.
#[derive(Debug, Clone, Default)]
pub struct Batch {
    pub rows: Vec<Vec<Option<String>>>,
    pub resume: Vec<u8>,
    pub last: bool,
    pub size: usize,
    pub epoch: u64,
    pub partition: u32,
}

/// An Error frame.
#[derive(Debug, Clone)]
pub struct Refused {
    pub code: ErrorCode,
    pub detail: u64,
    pub message: String,
}

/// What a read answered.
#[derive(Debug, Clone, Default)]
pub struct ReadAnswer {
    pub batches: Vec<Batch>,
    pub refused: Option<Refused>,
}

impl ReadAnswer {
    /// Every row of every batch.
    pub fn rows(&self) -> Vec<Vec<Option<String>>> {
        self.batches
            .iter()
            .flat_map(|batch| batch.rows.iter().cloned())
            .collect()
    }

    /// The answer's refusal.
    pub fn refusal(&self) -> TestResult<&Refused> {
        self.refused.as_ref().context("a refusal")
    }

    /// The rows, failing unless the read reached its last batch with no refusal.
    pub fn whole(&self) -> TestResult<Vec<Vec<Option<String>>>> {
        if let Some(refused) = &self.refused {
            return Err(TestError::new(format!("the read was refused: {refused:?}")));
        }
        match self.batches.last() {
            Some(batch) if batch.last => Ok(self.rows()),
            other => Err(TestError::new(format!("no last batch: {other:?}"))),
        }
    }
}

/// The cell's text: what it names by text, or a literal's inline lexical form.
fn cell_of(term: ResultTermView<'_>) -> String {
    let text = term.text().map(str::to_string).or_else(|| {
        let encoded = term.term()?;
        if let TermView::String(Text::Inline(inline)) = encoded.view().ok()? {
            Some(inline.to_string())
        } else {
            None
        }
    });
    text.unwrap_or_default()
}

/// `frame`, a Result, as a test reads it.
fn batch_of(frame: &ReceivedFrame) -> TestResult<Batch> {
    let view = frame.view()?;
    let result = ResultView::from_frame(&view)?;
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
    })
}

/// The next frame of `client`: a batch, or a refusal.
pub async fn next_of(client: &mut RoleClient) -> TestResult<Result<Batch, Refused>> {
    let frame = within(FRAME_WAIT, "compute's next frame", async {
        Ok(client.next_frame().await?)
    })
    .await?;
    let view = frame.view()?;
    if view.command() != Command::Error {
        return Ok(Ok(batch_of(&frame)?));
    }
    let error = ErrorReplyView::from_frame(&view)?;
    Ok(Err(Refused {
        code: error.code(),
        detail: error.detail(),
        message: error.message().to_string(),
    }))
}

/// Sends `read` on `client` and reads its answer, granting a frame's credit per batch.
pub async fn answer_on(client: &mut RoleClient, read: &ComputeRead) -> TestResult<ReadAnswer> {
    client.send_only(read).await?;
    let stream = client.last_stream();
    let mut answer = ReadAnswer::default();
    loop {
        match next_of(client).await? {
            Err(refused) => {
                answer.refused = Some(refused);
                return Ok(answer);
            }
            Ok(batch) => {
                let last = batch.last;
                answer.batches.push(batch);
                if last {
                    return Ok(answer);
                }
                client.send_on(stream, &Credit::new(1)).await?;
            }
        }
    }
}
