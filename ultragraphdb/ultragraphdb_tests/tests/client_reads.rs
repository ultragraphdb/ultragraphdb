/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a scenario's client writes and reads, in the wire's types, and a read's rows as a test compares them.

use ultragraphdb_client_compio::{
    ClientError, ClientErrorEnum, Committed, ResultStream, Transaction,
};
use ultragraphdb_core_terms::{TermView, Text};
use ultragraphdb_protocol_transport::TransportErrorEnum;
use ultragraphdb_protocol_wire::{
    DirectWrite, ErrorCode, PatternSlot, QuadPattern, Read, RequestTerm, ResultTermView,
    ResultView, StatementTarget, TermTable, Write, WriteChange,
};
use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::shells::TestClient;

/// The tenant the frontend's first start creates, the one every scenario writes in.
pub(crate) const TENANT: u64 = 0;

/// What a transaction writes besides what it read, so its commit is checked.
pub(crate) const OTHER: &str = "http://ex.org/other";

/// A language-free typed literal.
pub(crate) fn typed(lexical: &str, datatype: &str) -> RequestTerm {
    RequestTerm::Literal {
        lexical: lexical.to_string(),
        datatype: Some(datatype.to_string()),
    }
}

/// An `xsd:integer` literal.
pub(crate) fn integer(value: i64) -> RequestTerm {
    typed(
        &value.to_string(),
        "http://www.w3.org/2001/XMLSchema#integer",
    )
}

/// An IRI as a term.
pub(crate) fn iri(text: &str) -> RequestTerm {
    RequestTerm::Iri(text.to_string())
}

/// An `xsd:string` literal.
pub(crate) fn string(text: &str) -> RequestTerm {
    RequestTerm::Literal {
        lexical: text.to_string(),
        datatype: None,
    }
}

/// The insert of `subject` `predicate` `object` in the default graph, over a term table of its own.
fn inserting(
    subject: &str,
    predicate: &str,
    object: &RequestTerm,
) -> TestResult<(TermTable, Vec<WriteChange>)> {
    let mut table = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: table.iri(subject),
        predicate: table.iri(predicate),
        object: table.intern(object.clone())?,
    };
    Ok((table, vec![change]))
}

/// A Write of a transaction inserting `subject` `predicate` `object`.
pub(crate) fn write_of(subject: &str, predicate: &str, object: &RequestTerm) -> TestResult<Write> {
    let (table, changes) = inserting(subject, predicate, object)?;
    Ok(Write::new(table, changes))
}

/// A one-frame write inserting `subject` `predicate` `object`.
pub(crate) fn direct_of(
    subject: &str,
    predicate: &str,
    object: &RequestTerm,
) -> TestResult<DirectWrite> {
    let (table, changes) = inserting(subject, predicate, object)?;
    Ok(DirectWrite::new(TENANT, table, changes))
}

/// A one-frame write of the edge `subject` `predicate` `object` holding the property `key` of `value`.
pub(crate) fn edge_of(
    (subject, predicate, object): (&str, &str, &str),
    key: &str,
    value: &str,
) -> TestResult<DirectWrite> {
    let mut table = TermTable::new();
    let change = WriteChange::InsertEdge {
        graph: None,
        subject: table.iri(subject),
        predicate: table.iri(predicate),
        object: table.iri(object),
        properties: vec![(table.iri(key), table.intern(string(value))?)],
    };
    Ok(DirectWrite::new(TENANT, table, vec![change]))
}

/// A one-frame write setting the property `key` of `value` on the statement `reifier` names.
pub(crate) fn property_of(reifier: &str, key: &str, value: &str) -> TestResult<DirectWrite> {
    let mut table = TermTable::new();
    let change = WriteChange::SetProperty {
        statement: StatementTarget::Reifier {
            graph: None,
            reifier: table.iri(reifier),
        },
        predicate: table.iri(key),
        value: table.intern(string(value))?,
    };
    Ok(DirectWrite::new(TENANT, table, vec![change]))
}

/// The graph position of a pattern.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum GraphAt {
    /// The default graph.
    #[default]
    Default,
    /// Any graph: the position is unbound, and a column of the rows.
    Any,
    /// The graph of this IRI.
    Named(String),
}

/// A quad pattern, its graph the default unless said, each other position bound to the term given or unbound.
#[derive(Debug, Clone, Default)]
pub(crate) struct Pattern {
    pub(crate) graph: GraphAt,
    pub(crate) subject: Option<RequestTerm>,
    pub(crate) predicate: Option<String>,
    pub(crate) object: Option<RequestTerm>,
}

impl Pattern {
    /// The pattern binding `subject` and `predicate` where given, its object unbound.
    pub(crate) fn of(subject: Option<&str>, predicate: Option<&str>) -> Self {
        Self {
            graph: GraphAt::Default,
            subject: subject.map(iri),
            predicate: predicate.map(str::to_string),
            object: None,
        }
    }

    /// The pattern with its graph position `graph`.
    pub(crate) fn in_graph(mut self, graph: GraphAt) -> Self {
        self.graph = graph;
        self
    }

    /// The pattern with its subject bound to `subject`, whatever kind of term it is.
    pub(crate) fn with_subject(mut self, subject: RequestTerm) -> Self {
        self.subject = Some(subject);
        self
    }

    /// The pattern with its object bound to `object`.
    pub(crate) fn with_object(mut self, object: RequestTerm) -> Self {
        self.object = Some(object);
        self
    }

    /// The Read of the pattern in `transaction`, at most `limit` rows from `resume`.
    pub(crate) fn read(
        &self,
        transaction: &Transaction,
        limit: u32,
        resume: Vec<u8>,
    ) -> TestResult<Read> {
        let mut terms = TermTable::new();
        let graph = match &self.graph {
            GraphAt::Default => PatternSlot::DefaultGraph,
            GraphAt::Any => PatternSlot::Unbound,
            GraphAt::Named(text) => PatternSlot::Bound(terms.iri(text)),
        };
        let subject = match &self.subject {
            Some(term) => PatternSlot::Bound(terms.intern(term.clone())?),
            None => PatternSlot::Unbound,
        };
        let predicate = self
            .predicate
            .as_ref()
            .map_or(PatternSlot::Unbound, |text| {
                PatternSlot::Bound(terms.iri(text))
            });
        let object = match &self.object {
            Some(term) => PatternSlot::Bound(terms.intern(term.clone())?),
            None => PatternSlot::Unbound,
        };
        let pattern = QuadPattern::new(graph, subject, predicate, object);
        Ok(Read::new(
            transaction.stream(),
            limit,
            terms,
            pattern,
            resume,
        ))
    }
}

/// A read's rows, each cell the text of what it names, empty for a literal or an absent cell, and its resume key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Answer {
    pub(crate) rows: Vec<Vec<String>>,
    pub(crate) resume: Vec<u8>,
}

impl Answer {
    /// Column `column` of every row.
    pub(crate) fn column(&self, column: usize) -> Vec<String> {
        self.rows
            .iter()
            .map(|row| row.get(column).cloned().unwrap_or_default())
            .collect()
    }
}

/// What a cell's term says: its text, or the lexical form an inline string carries by value.
pub(crate) fn text_of(term: &ResultTermView<'_>) -> String {
    term.text()
        .map(str::to_string)
        .or_else(|| {
            let encoded = term.term()?;
            if let TermView::String(Text::Inline(inline)) = encoded.view().ok()? {
                Some(inline.to_string())
            } else {
                None
            }
        })
        .unwrap_or_default()
}

/// The rows of `batch`, and its resume key.
pub(crate) fn batch_of(batch: &ResultView<'_>) -> (Vec<Vec<String>>, Vec<u8>) {
    let rows = (0..batch.rows())
        .map(|row| {
            (0..batch.columns())
                .map(|column| {
                    batch
                        .cell(column, row)
                        .and_then(|cell| batch.term(cell))
                        .map(|term| text_of(&term))
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect();
    (rows, batch.resume().to_vec())
}

/// Reads `pattern` in `transaction` to its `LAST`, at most `limit` rows from `resume`.
pub(crate) async fn read(
    client: &mut TestClient,
    transaction: &Transaction,
    pattern: &Pattern,
    (limit, resume): (u32, Vec<u8>),
) -> TestResult<Answer> {
    let stream = client
        .read(transaction, &pattern.read(transaction, limit, resume)?)
        .await?;
    drain(client, &stream).await
}

/// Drains the read `stream` to its `LAST`.
pub(crate) async fn drain(client: &mut TestClient, stream: &ResultStream) -> TestResult<Answer> {
    let mut answer = Answer::default();
    while let Some((rows, key)) = client.next_batch(stream, batch_of).await? {
        answer.rows.extend(rows);
        answer.resume = key;
    }
    Ok(answer)
}

/// Reads `pattern` whole in `transaction`.
pub(crate) async fn read_whole(
    client: &mut TestClient,
    transaction: &Transaction,
    pattern: &Pattern,
) -> TestResult<Answer> {
    read(client, transaction, pattern, (0, Vec::new())).await
}

/// What a scene saw, for its `then` step to judge: the answers of its reads, the refusals it was given and the commits it made,
/// each in the order they came, and anything else as text.
#[derive(Debug, Default)]
pub(crate) struct Seen {
    pub(crate) answers: Vec<Answer>,
    pub(crate) refusals: Vec<ClientError>,
    pub(crate) commits: Vec<Committed>,
    pub(crate) facts: Vec<String>,
}

/// The code, detail and message of a refusal on a stream; a failure naming anything else.
pub(crate) fn refusal_of(error: &ClientError) -> TestResult<(ErrorCode, u64, String)> {
    if let ClientErrorEnum::Refused {
        code,
        detail,
        message,
        ..
    } = &error.0
    {
        Ok((*code, *detail, message.clone()))
    } else {
        Err(TestError::new(format!(
            "a refusal on a stream: {:?}",
            error.0
        )))
    }
}

/// The code of the Error that closed the connection or refused its handshake.
pub(crate) fn closing_code(error: &ClientError) -> Option<ErrorCode> {
    if let ClientErrorEnum::Closed { code, .. } = &error.0 {
        return Some(*code);
    }
    if let ClientErrorEnum::Transport { error } = &error.0
        && let TransportErrorEnum::Refused { code, .. } = &error.0
    {
        return Some(*code);
    }
    if let ClientErrorEnum::ConnectionLost { cause: Some(cause) } = &error.0 {
        return closing_code(cause);
    }
    None
}

/// That `answer` failed with a refusal of `code`, whose detail and message it returns.
pub(crate) fn refused_with<T: std::fmt::Debug>(
    answer: Result<T, ClientError>,
    code: ErrorCode,
) -> TestResult<(u64, String)> {
    match answer {
        Ok(value) => Err(TestError::new(format!(
            "code {code:?} expected, answered {value:?}"
        ))),
        Err(error) => {
            let (found, detail, message) = refusal_of(&error)?;
            if found == code {
                Ok((detail, message))
            } else {
                Err(TestError::new(format!(
                    "code {code:?} expected, refused {error:?}"
                )))
            }
        }
    }
}

/// Writes something `first` did not read, then commits it: the conflict kind refusing the commit, `None` for a commit that
/// holds.
pub(crate) async fn commit_a_write(
    client: &mut TestClient,
    first: Transaction,
) -> TestResult<Option<u64>> {
    let write = write_of("http://ex.org/unrelated", OTHER, &string("x"))?;
    client.write(&first, &write).await?;
    match client.commit(first, None).await {
        Ok(_) => Ok(None),
        Err(ClientError(ClientErrorEnum::Refused {
            code: ErrorCode::Conflict,
            detail,
            ..
        })) => Ok(Some(detail)),
        Err(other) => Err(TestError::new(format!(
            "the commit was refused for another reason: {other}"
        ))),
    }
}
