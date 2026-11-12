/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A ComputeRead as the frontend sends it, and its answer as the test reads it.

use std::time::Duration;

use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_terms::{TermView, Text};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    BodyView, Command, ComputeRead, Credit, ErrorCode, ErrorReplyView, FLAG_LAST,
    MAX_FRAME_SIZE_U32, PatternSlot, QuadPattern, ReadLevel, RequestTerm, ResultTermView,
    ResultView, TermTable,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// How long the test waits on one frame of compute's: past every wait the played roles are given.
pub const ANSWER_WAIT: Duration = Duration::from_secs(10);

/// How long a read's transaction has, in milliseconds.
pub const DEADLINE_MS: u32 = 60_000;

/// A term of a Result, as a test compares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// What its encoding names by hash, or a string's inline lexical form; empty for neither.
    pub text: String,
    /// Whether it is tagged a blank node.
    pub blank: bool,
    /// Its annotation: a language tag or a datatype's IRI held by hash.
    pub annotation: Option<String>,
    /// Its encoding.
    pub encoding: Vec<u8>,
}

/// One Result frame.
#[derive(Debug, Clone, Default)]
pub struct Batch {
    /// Its rows, each its cells in column order.
    pub rows: Vec<Vec<Option<Cell>>>,
    /// Its resume key.
    pub resume: Vec<u8>,
    /// Whether it is flagged `LAST`.
    pub last: bool,
    /// The epoch it names.
    pub epoch: u64,
    /// The partition it names.
    pub partition: u32,
    /// Its columns.
    pub columns: u32,
}

/// An Error frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// Its code.
    pub code: ErrorCode,
    /// Its detail.
    pub detail: u64,
    /// Its message.
    pub message: String,
}

/// What a read answered.
#[derive(Debug, Clone, Default)]
pub struct ReadAnswer {
    /// Its batches, in order.
    pub batches: Vec<Batch>,
    /// Its refusal, which ended it.
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

    /// Column `column` of every row, by its text.
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

    /// The last batch, failing unless the read reached its `LAST` with no refusal.
    pub fn whole(&self) -> TestResult<&Batch> {
        if let Some(refused) = &self.refused {
            return Err(TestError::new(format!("the read was refused: {refused:?}")));
        }
        match self.batches.last() {
            Some(batch) if batch.last => Ok(batch),
            other => Err(TestError::new(format!("no last batch: {other:?}"))),
        }
    }

    /// Fails unless the read reached its `LAST` with no refusal.
    pub fn ended(&self) -> TestResult {
        self.whole().map(|_| ())
    }

    /// The refusal, failing for an answer that has none.
    pub fn refusal(&self) -> TestResult<&Refused> {
        self.refused.as_ref().context("a refusal")
    }
}

/// The term's cell.
fn cell_of(term: ResultTermView<'_>) -> Cell {
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
        blank: term.is_blank_node(),
        annotation: term.annotation().map(str::to_string),
        encoding: term.encoding().to_vec(),
    }
}

/// `frame`, a Result, as the test reads it.
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
        epoch: result.epoch(),
        partition: result.partition(),
        columns: result.columns(),
    })
}

/// The Error `frame` holds, if it is one.
fn refusal_of(frame: &ReceivedFrame) -> TestResult<Option<Refused>> {
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

/// The next frame of `client`, failing past [`ANSWER_WAIT`].
pub async fn next_frame(client: &mut RoleClient) -> TestResult<ReceivedFrame> {
    compio::time::timeout(ANSWER_WAIT, client.next_frame())
        .await
        .map_err(|_| TestError::new(format!("no frame came within {ANSWER_WAIT:?}")))?
        .map_err(TestError::from)
}

/// Sends `read` on `client` and reads its answer to its end, granting a frame of credit per batch.
pub async fn answer_on(client: &mut RoleClient, read: &ComputeRead) -> TestResult<ReadAnswer> {
    client.send_only(read).await?;
    let stream = client.last_stream();
    let mut answer = ReadAnswer::default();
    loop {
        let frame = next_frame(client).await?;
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

/// A quad pattern by its terms: `None` unbound, a graph `Some(None)` the default graph.
#[derive(Debug, Clone, Default)]
pub struct Pattern {
    /// The graph: `None` unbound, `Some(None)` the default graph, `Some(Some(iri))` a named one.
    pub graph: Option<Option<String>>,
    /// The subject's IRI.
    pub subject: Option<String>,
    /// The predicate's IRI.
    pub predicate: Option<String>,
    /// The object.
    pub object: Option<RequestTerm>,
}

impl Pattern {
    /// The pattern of the default graph.
    pub fn of(subject: Option<&str>, predicate: Option<&str>, object: Option<RequestTerm>) -> Self {
        Self {
            graph: Some(None),
            subject: subject.map(str::to_string),
            predicate: predicate.map(str::to_string),
            object,
        }
    }

    /// The pattern's slots over a new term table.
    pub fn table(&self) -> TestResult<(TermTable, QuadPattern)> {
        let mut terms = TermTable::new();
        let graph = match &self.graph {
            None => PatternSlot::Unbound,
            Some(None) => PatternSlot::DefaultGraph,
            Some(Some(iri)) => PatternSlot::Bound(terms.iri(iri)),
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

    /// The read of the pattern in `tenant` at `level`, the client's frame and credit generous, nothing else set.
    pub fn read(&self, tenant: u64, read_only: bool, level: ReadLevel) -> TestResult<ComputeRead> {
        let (terms, slots) = self.table()?;
        Ok(
            ComputeRead::new(tenant, read_only, level, terms, slots, Vec::new())
                .with_principal(1)
                .with_deadline_ms(DEADLINE_MS)
                .with_client_limits(MAX_FRAME_SIZE_U32, 4),
        )
    }
}

/// An IRI as a term.
pub fn iri(text: &str) -> RequestTerm {
    RequestTerm::Iri(text.to_string())
}
