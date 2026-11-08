/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A ComputeRead as the frontend sends one, and its answer as the frontend reads it: every Result batch's rows by their terms,
//! or the Error that ended the stream.

use std::time::Duration;

use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_wire::{
    ABSENT, BodyView, Cancel, Command, ComputeRead, Credit, ErrorCode, ErrorReplyView, FLAG_LAST,
    PatternSlot, QuadPattern, ReadLevel, RequestTerm, ResultView, TermTable,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use super::played_data::TENANT;
use super::played_partition::{EPOCH, version};

/// The longest a test waits on any one frame of a read's answer.
const FRAME_WAIT: Duration = Duration::from_secs(10);

/// The largest frame a played client takes, the protocol's largest.
const MAX_FRAME: u32 = 1 << 20;

/// The most rows a played read answers: one past it is a read that does not end.
const ROWS_MAX: usize = 10_000;

/// The credit a read's stream starts with.
const CREDIT: u32 = 16;

/// The bound positions of a pattern in the default graph: an unbound one `None`.
#[derive(Debug, Clone, Default)]
pub struct Bound {
    /// The subject's IRI.
    pub subject: Option<String>,
    /// The predicate's IRI.
    pub predicate: Option<String>,
    /// The object.
    pub object: Option<RequestTerm>,
}

/// A read of `bound` in the default graph of the played tenant, read-only when `read_only`, at `level`, from `resume`; every
/// Result frame within the protocol's largest, a deadline of 10 s.
pub fn read_of(
    bound: &Bound,
    read_only: bool,
    level: ReadLevel,
    resume: Vec<u8>,
) -> TestResult<ComputeRead> {
    let mut terms = TermTable::new();
    let mut iri = |held: &Option<String>| {
        held.as_deref().map_or(PatternSlot::Unbound, |iri| {
            PatternSlot::Bound(terms.iri(iri))
        })
    };
    let (subject, predicate) = (iri(&bound.subject), iri(&bound.predicate));
    let object = match &bound.object {
        Some(term) => PatternSlot::Bound(terms.intern(term.clone())?),
        None => PatternSlot::Unbound,
    };
    let pattern = QuadPattern::new(PatternSlot::DefaultGraph, subject, predicate, object);
    Ok(
        ComputeRead::new(TENANT, read_only, level, terms, pattern, resume)
            .with_client_limits(MAX_FRAME, CREDIT)
            .with_deadline_ms(10_000),
    )
}

/// A read-only read of `bound` at the snapshot of the played epoch and version.
pub fn snapshot_of(bound: &Bound) -> TestResult<ComputeRead> {
    snapshot_from(bound, Vec::new())
}

/// A read-only read of `bound` at the snapshot of the played epoch and version, from `resume`.
pub fn snapshot_from(bound: &Bound, resume: Vec<u8>) -> TestResult<ComputeRead> {
    Ok(read_of(bound, true, ReadLevel::Snapshot, resume)?
        .with_read_version(version())
        .with_epoch(EPOCH))
}

/// A term of a Result batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The text it names: an IRI's, a blank node's skolem IRI, a hashed literal's.
    pub text: Option<String>,
    /// Its encoding as the batch holds it.
    pub encoding: Vec<u8>,
}

/// One Result batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    /// The cluster its header names.
    pub cluster: u64,
    /// The epoch it was read at.
    pub epoch: u64,
    /// The partition the read reached alone, 0 for several.
    pub partition: u32,
    /// Its rows, each cell `None` where absent.
    pub rows: Vec<Vec<Option<Cell>>>,
    /// Its resume key.
    pub resume: Vec<u8>,
}

/// A read's answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadOutcome {
    /// Its batches, in order.
    pub batches: Vec<Batch>,
    /// The refusal that ended it: its code and message.
    pub refusal: Option<(ErrorCode, String)>,
}

impl ReadOutcome {
    /// The answer, refused if it was.
    pub fn answered(self) -> TestResult<Self> {
        match &self.refusal {
            Some((code, message)) => Err(TestError::new(format!(
                "the read was refused, {code:?}: {message}"
            ))),
            None => Ok(self),
        }
    }

    /// The texts of column `column` of every row, in order.
    pub fn column(&self, column: usize) -> Vec<Option<String>> {
        self.batches
            .iter()
            .flat_map(|batch| batch.rows.iter())
            .map(|row| {
                row.get(column)
                    .cloned()
                    .flatten()
                    .and_then(|cell| cell.text)
            })
            .collect()
    }

    /// Every row's texts, in order.
    pub fn texts(&self) -> Vec<Vec<Option<String>>> {
        self.rows()
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|cell| cell.and_then(|cell| cell.text))
                    .collect()
            })
            .collect()
    }

    /// Every row's cells, in order.
    pub fn rows(&self) -> Vec<Vec<Option<Cell>>> {
        self.batches
            .iter()
            .flat_map(|batch| batch.rows.iter().cloned())
            .collect()
    }

    /// The last batch's resume key.
    pub fn resume(&self) -> Vec<u8> {
        self.batches
            .last()
            .map(|batch| batch.resume.clone())
            .unwrap_or_default()
    }
}

/// Sends `read` on a stream of its own: the stream.
pub async fn send(client: &mut RoleClient, read: &ComputeRead) -> TestResult<u64> {
    client.send_only(read).await?;
    Ok(client.last_stream())
}

/// Cancels the read on `stream`.
pub async fn cancel(client: &mut RoleClient, stream: u64) -> TestResult {
    Ok(client.send_on(stream, &Cancel).await?)
}

/// Sends `read` and reads its answer to its last frame, granting a frame of credit for each it takes.
pub async fn read(client: &mut RoleClient, read: &ComputeRead) -> TestResult<ReadOutcome> {
    let stream = send(client, read).await?;
    answer_of(client, stream).await
}

/// The answer on `stream`, to its last frame.
pub async fn answer_of(client: &mut RoleClient, stream: u64) -> TestResult<ReadOutcome> {
    let mut outcome = ReadOutcome::default();
    loop {
        let frame = compio::time::timeout(FRAME_WAIT, client.next_frame())
            .await
            .map_err(|_| TestError::new("no frame of the read's answer came within 10 s"))??;
        let view = frame.view()?;
        if view.header().stream() != stream {
            return Err(TestError::new(format!(
                "a frame of stream {}",
                view.header().stream()
            )));
        }
        let last = view.header().flags() & FLAG_LAST != 0;
        if view.command() == Command::Error {
            let error = ErrorReplyView::from_frame(&view)?;
            outcome.refusal = Some((error.code(), error.message().to_string()));
            return Ok(outcome);
        }
        let result = ResultView::from_frame(&view)?;
        outcome
            .batches
            .push(batch_of(view.header().cluster(), &result)?);
        if outcome
            .batches
            .iter()
            .map(|batch| batch.rows.len())
            .sum::<usize>()
            > ROWS_MAX
        {
            return Err(TestError::new(format!(
                "the read answered more than {ROWS_MAX} rows"
            )));
        }
        drop(frame);
        if last {
            return Ok(outcome);
        }
        client.send_on(stream, &Credit::new(1)).await?;
    }
}

/// The batch `result` holds.
fn batch_of(cluster: u64, result: &ResultView<'_>) -> TestResult<Batch> {
    let mut rows = Vec::new();
    for row in 0..result.rows() {
        let mut cells = Vec::new();
        for column in 0..result.columns() {
            let index = result.cell(column, row).context("a cell of the batch")?;
            let cell = match (index != ABSENT).then_some(index) {
                Some(index) => {
                    let term = result.term(index).context("a cell's term")?;
                    Some(Cell {
                        text: term.text().map(str::to_string),
                        encoding: term.encoding().to_vec(),
                    })
                }
                None => None,
            };
            cells.push(cell);
        }
        rows.push(cells);
    }
    Ok(Batch {
        cluster,
        epoch: result.epoch(),
        partition: result.partition(),
        rows,
        resume: result.resume().to_vec(),
    })
}
