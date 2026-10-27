/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Changes and declarations as records: 16 bytes each, `kind` u8, `flags` u8, two zero bytes, `args_start` u32 and
//! `args_len` u32 into an args section of u32s, and `extra` u32; strings in a string list. The args of change `i`
//! follow those of change `i − 1`. Terms are N-Triples strings, the IRIs a declaration names plain IRI strings.
//!
//! | Kind | Change | Flags | Args |
//! |---|---|---|---|
//! | 1 | Insert | — | graph or [`crate::ABSENT`], subject, predicate, object |
//! | 2 | InsertEdge | — | graph, subject, predicate, object, then a property and its value per pair |
//! | 3 | Delete | — | graph, subject, predicate, object |
//! | 4 | DeleteStatement | — | graph, the statement ID's low and high u32 |
//! | 5 | DeletePattern | — | graph, subject, predicate |
//! | 6 | SetProperty | bit 0: by triple | the statement (graph, ID low, ID high; or graph, subject, predicate, object), predicate, value |
//! | 7 | DeleteProperty | bit 0: by triple | the statement, predicate, value or [`crate::ABSENT`] |
//! | 8 | Declare a predicate | bits 0–1: kind (0 edge, 1 property, 2 either); bit 2: single; bit 3: unique; bit 4: count index; bit 5: vector index | predicate, datatype or [`crate::ABSENT`], the tokenizer count and names, the domain count and IRIs, and with a vector index its factory, the option count and each option's name and value |
//! | 9 | Declare an entity type | bit 0: abstract | IRI, parent or [`crate::ABSENT`] |
//! | 10 | Declare a relation | — | IRI, then its roles |
//!
//! `extra` is zero but in the data dictionary's answer, where an entity type's holds its preorder number.

use oxrdf::GraphName;
use ultragraphdb_core_mapping::{
    Cardinality, Change, Changeset, Declaration, PredicateDeclaration, PredicateKind,
    RelationDeclaration, StatementId, StatementRef, TypeDeclaration, VectorIndexSpec,
};
use ultragraphdb_error_protocol::ProtocolError;

use crate::types::string_list::StringList;
use crate::utils::constants::ABSENT;
use crate::utils::sections::{as_u32, records as whole_records};
use crate::utils::string_writer::StringWriter;
use crate::utils::terms::{
    graph_text, parse_graph, parse_iri, parse_named, parse_subject, parse_term,
};

/// The size of a change record.
pub(crate) const CHANGE_RECORD_SIZE: usize = 16;

const INSERT: u8 = 1;
const INSERT_EDGE: u8 = 2;
const DELETE: u8 = 3;
const DELETE_STATEMENT: u8 = 4;
const DELETE_PATTERN: u8 = 5;
const SET_PROPERTY: u8 = 6;
const DELETE_PROPERTY: u8 = 7;
const PREDICATE: u8 = 8;
const ENTITY_TYPE: u8 = 9;
const RELATION: u8 = 10;

const BY_TRIPLE: u8 = 1;
const SINGLE: u8 = 1 << 2;
const UNIQUE: u8 = 1 << 3;
const COUNT_INDEX: u8 = 1 << 4;
const VECTOR_INDEX: u8 = 1 << 5;
const ABSTRACT: u8 = 1;

/// Records being written, with their args and strings.
#[derive(Debug, Clone, Default)]
pub(crate) struct ChangeWriter {
    pub(crate) records: Vec<u8>,
    pub(crate) args: Vec<u32>,
    pub(crate) strings: StringWriter,
}

impl ChangeWriter {
    fn string(&mut self, s: &str) -> u32 {
        self.strings.intern(s)
    }

    fn term(&mut self, term: &impl std::fmt::Display) -> u32 {
        self.strings.intern(&term.to_string())
    }

    fn graph(&mut self, graph: &GraphName) -> u32 {
        graph_text(graph).map_or(ABSENT, |text| self.strings.intern(&text))
    }

    fn record(&mut self, kind: u8, flags: u8, args: Vec<u32>, extra: u32) {
        let start = as_u32(self.args.len());
        let len = as_u32(args.len());
        self.args.extend(args);
        self.records.extend_from_slice(&[kind, flags, 0, 0]);
        self.records.extend_from_slice(&start.to_le_bytes());
        self.records.extend_from_slice(&len.to_le_bytes());
        self.records.extend_from_slice(&extra.to_le_bytes());
    }

    /// The args naming `statement`, and whether it is named by its triple.
    fn statement(&mut self, statement: &StatementRef) -> (Vec<u32>, u8) {
        match statement {
            StatementRef::Id { graph, id } => {
                let graph = self.graph(graph);
                (vec![graph, id.value() as u32, (id.value() >> 32) as u32], 0)
            }
            StatementRef::Triple {
                graph,
                subject,
                predicate,
                object,
            } => (
                vec![
                    self.graph(graph),
                    self.term(subject),
                    self.term(predicate),
                    self.term(object),
                ],
                BY_TRIPLE,
            ),
        }
    }

    /// Appends `change`'s record.
    pub(crate) fn change(&mut self, change: &Change) {
        match change {
            Change::Insert {
                graph,
                subject,
                predicate,
                object,
            }
            | Change::Delete {
                graph,
                subject,
                predicate,
                object,
            } => {
                let kind = if matches!(change, Change::Insert { .. }) {
                    INSERT
                } else {
                    DELETE
                };
                let args = vec![
                    self.graph(graph),
                    self.term(subject),
                    self.term(predicate),
                    self.term(object),
                ];
                self.record(kind, 0, args, 0);
            }
            Change::InsertEdge {
                graph,
                subject,
                predicate,
                object,
                properties,
            } => {
                let mut args = vec![
                    self.graph(graph),
                    self.term(subject),
                    self.term(predicate),
                    self.term(object),
                ];
                for (property, value) in properties {
                    args.push(self.term(property));
                    args.push(self.term(value));
                }
                self.record(INSERT_EDGE, 0, args, 0);
            }
            Change::DeleteStatement { graph, id } => {
                let args = vec![
                    self.graph(graph),
                    id.value() as u32,
                    (id.value() >> 32) as u32,
                ];
                self.record(DELETE_STATEMENT, 0, args, 0);
            }
            Change::DeletePattern {
                graph,
                subject,
                predicate,
            } => {
                let args = vec![self.graph(graph), self.term(subject), self.term(predicate)];
                self.record(DELETE_PATTERN, 0, args, 0);
            }
            Change::SetProperty {
                statement,
                predicate,
                value,
            } => {
                let (mut args, flags) = self.statement(statement);
                args.push(self.term(predicate));
                args.push(self.term(value));
                self.record(SET_PROPERTY, flags, args, 0);
            }
            Change::DeleteProperty {
                statement,
                predicate,
                value,
            } => {
                let (mut args, flags) = self.statement(statement);
                args.push(self.term(predicate));
                let value = value.as_ref().map_or(ABSENT, |v| self.term(v));
                args.push(value);
                self.record(DELETE_PROPERTY, flags, args, 0);
            }
            Change::Declare(declaration) => self.declaration(declaration, 0),
        }
    }

    /// Appends `declaration`'s record with `extra`.
    pub(crate) fn declaration(&mut self, declaration: &Declaration, extra: u32) {
        match declaration {
            Declaration::Predicate(p) => {
                let kind = match p.kind() {
                    PredicateKind::Edge => 0,
                    PredicateKind::Property => 1,
                    PredicateKind::Either => 2,
                };
                let flags =
                    kind | if p.cardinality() == Cardinality::Single {
                        SINGLE
                    } else {
                        0
                    } | if p.unique() { UNIQUE } else { 0 }
                        | if p.count_index() { COUNT_INDEX } else { 0 }
                        | if p.vector_index().is_some() {
                            VECTOR_INDEX
                        } else {
                            0
                        };
                let mut args = vec![self.string(p.predicate().as_str())];
                let datatype = p.datatype().map_or(ABSENT, |d| self.string(d.as_str()));
                args.push(datatype);
                args.push(as_u32(p.tokenizers().len()));
                for tokenizer in p.tokenizers() {
                    args.push(self.string(tokenizer));
                }
                args.push(as_u32(p.domains().len()));
                for domain in p.domains() {
                    args.push(self.string(domain.as_str()));
                }
                if let Some(spec) = p.vector_index() {
                    args.push(self.string(spec.factory()));
                    args.push(as_u32(spec.options().len()));
                    for (name, value) in spec.options() {
                        args.push(self.string(name));
                        args.push(self.string(value));
                    }
                }
                self.record(PREDICATE, flags, args, extra);
            }
            Declaration::EntityType(t) => {
                let mut args = vec![self.string(t.iri().as_str())];
                let parent = t.parent().map_or(ABSENT, |p| self.string(p.as_str()));
                args.push(parent);
                self.record(
                    ENTITY_TYPE,
                    if t.is_abstract() { ABSTRACT } else { 0 },
                    args,
                    extra,
                );
            }
            Declaration::Relation(r) => {
                let mut args = vec![self.string(r.iri().as_str())];
                for role in r.roles() {
                    args.push(self.string(role.as_str()));
                }
                self.record(RELATION, 0, args, extra);
            }
        }
    }
}

/// A change record's fields.
#[derive(Debug, Clone, Copy)]
struct Record {
    kind: u8,
    flags: u8,
    zero: u16,
    start: u32,
    len: u32,
    extra: u32,
}

fn record_at(records: &[u8], index: usize) -> Option<Record> {
    let r = records.get(index * CHANGE_RECORD_SIZE..(index + 1) * CHANGE_RECORD_SIZE)?;
    let u32_at = |at: usize| {
        r.get(at..at + 4)
            .and_then(|b| b.try_into().ok())
            .map_or(0, u32::from_le_bytes)
    };
    Some(Record {
        kind: *r.first()?,
        flags: *r.get(1)?,
        zero: u16::from_le_bytes([*r.get(2)?, *r.get(3)?]),
        start: u32_at(4),
        len: u32_at(8),
        extra: u32_at(12),
    })
}

/// The flags each kind defines.
fn allowed_flags(kind: u8) -> u8 {
    match kind {
        SET_PROPERTY | DELETE_PROPERTY => BY_TRIPLE,
        PREDICATE => 0b11 | SINGLE | UNIQUE | COUNT_INDEX | VECTOR_INDEX,
        ENTITY_TYPE => ABSTRACT,
        _ => 0,
    }
}

/// A check of one record's args, `run`, which start at arg `base`.
struct Run<'r> {
    command: u16,
    section: usize,
    base: usize,
    run: &'r [u32],
    strings: u32,
}

impl Run<'_> {
    fn count(&self, expected: usize) -> Result<(), ProtocolError> {
        if self.run.len() == expected {
            Ok(())
        } else {
            Err(self.mismatch(expected))
        }
    }

    fn mismatch(&self, expected: usize) -> ProtocolError {
        ProtocolError::CountMismatch(
            self.command,
            "change args",
            self.run.len() as u64,
            expected as u64,
        )
    }

    /// Arg `i` names a string.
    fn string(&self, i: usize) -> Result<(), ProtocolError> {
        let value = self.run.get(i).copied().unwrap_or(ABSENT);
        if value < self.strings {
            Ok(())
        } else {
            Err(ProtocolError::IndexOutOfRange(
                self.command,
                self.section,
                self.base + i,
                value,
                self.strings,
            ))
        }
    }

    /// Arg `i` names a string or is absent.
    fn optional(&self, i: usize) -> Result<(), ProtocolError> {
        if self.run.get(i) == Some(&ABSENT) {
            Ok(())
        } else {
            self.string(i)
        }
    }

    fn strings(&self, range: std::ops::Range<usize>) -> Result<(), ProtocolError> {
        range.into_iter().try_for_each(|i| self.string(i))
    }

    /// A count at arg `at` followed by that many strings; the arg after them.
    fn counted(&self, at: usize, per: usize) -> Result<usize, ProtocolError> {
        let n = *self.run.get(at).ok_or_else(|| self.mismatch(at + 1))? as usize;
        let end = (at + 1).saturating_add(n.saturating_mul(per));
        if end > self.run.len() {
            return Err(self.mismatch(end));
        }
        self.strings(at + 1..end)?;
        Ok(end)
    }

    fn check(&self, kind: u8, flags: u8) -> Result<(), ProtocolError> {
        let len = self.run.len();
        match kind {
            INSERT | DELETE => {
                self.count(4)?;
                self.optional(0)?;
                self.strings(1..4)
            }
            INSERT_EDGE => {
                if len < 4 || !(len - 4).is_multiple_of(2) {
                    return Err(self.mismatch(if len < 4 { 4 } else { len + 1 }));
                }
                self.optional(0)?;
                self.strings(1..len)
            }
            DELETE_STATEMENT => {
                self.count(3)?;
                self.optional(0)
            }
            DELETE_PATTERN => {
                self.count(3)?;
                self.optional(0)?;
                self.strings(1..3)
            }
            SET_PROPERTY | DELETE_PROPERTY => {
                let by_triple = flags & BY_TRIPLE != 0;
                let statement = if by_triple { 4 } else { 3 };
                self.count(statement + 2)?;
                self.optional(0)?;
                if by_triple {
                    self.strings(1..4)?;
                }
                self.string(statement)?;
                if kind == SET_PROPERTY {
                    self.string(statement + 1)
                } else {
                    self.optional(statement + 1)
                }
            }
            PREDICATE => {
                if len < 3 {
                    return Err(self.mismatch(3));
                }
                self.string(0)?;
                self.optional(1)?;
                let after_tokenizers = self.counted(2, 1)?;
                let mut at = self.counted(after_tokenizers, 1)?;
                if flags & VECTOR_INDEX != 0 {
                    if at + 2 > len {
                        return Err(self.mismatch(at + 2));
                    }
                    self.string(at)?;
                    at = self.counted(at + 1, 2)?;
                }
                self.count(at)
            }
            ENTITY_TYPE => {
                self.count(2)?;
                self.string(0)?;
                self.optional(1)
            }
            _ => {
                if len == 0 {
                    return Err(self.mismatch(1));
                }
                self.strings(0..len)
            }
        }
    }
}

/// The records of a body checked: whole records, kinds known (only declarations when `declarations_only`, in the
/// order predicates, entity types, relations), flags and zero bytes as the kind defines, args contiguous and consumed
/// exactly, each string index in range. `sections` are the records', the args' and the strings' text section, for
/// the errors.
pub(crate) fn check_records(
    command: u16,
    sections: (usize, usize, usize),
    records: &[u8],
    args: &[u32],
    strings: &StringList<'_>,
    declarations_only: bool,
) -> Result<(), ProtocolError> {
    let count = whole_records(command, sections.0, records, CHANGE_RECORD_SIZE)?.len();
    let mut next = 0usize;
    let mut last_kind = 0u8;
    for index in 0..count {
        let Some(r) = record_at(records, index) else {
            break;
        };
        let known = if declarations_only {
            (PREDICATE..=RELATION).contains(&r.kind)
        } else {
            (INSERT..=RELATION).contains(&r.kind)
        };
        if !known {
            return Err(ProtocolError::FieldValue(
                command,
                "change kind",
                r.kind.into(),
            ));
        }
        if declarations_only && r.kind < last_kind {
            return Err(ProtocolError::FieldValue(
                command,
                "declaration order",
                r.kind.into(),
            ));
        }
        last_kind = r.kind;
        let flags_invalid = r.flags & !allowed_flags(r.kind) != 0
            || (r.kind == PREDICATE && r.flags & 0b11 == 0b11);
        if flags_invalid {
            return Err(ProtocolError::FieldValue(command, "flags", r.flags.into()));
        }
        if r.zero != 0 {
            return Err(ProtocolError::FieldValue(
                command,
                "change record",
                r.zero.into(),
            ));
        }
        if r.extra != 0 && !(declarations_only && r.kind == ENTITY_TYPE) {
            return Err(ProtocolError::FieldValue(command, "extra", r.extra.into()));
        }
        if r.start as usize != next {
            return Err(ProtocolError::FieldValue(
                command,
                "args_start",
                r.start.into(),
            ));
        }
        let end = next.saturating_add(r.len as usize);
        let run = args.get(next..end).ok_or_else(|| {
            ProtocolError::CountMismatch(command, "args", args.len() as u64, end as u64)
        })?;
        Run {
            command,
            section: sections.1,
            base: next,
            run,
            strings: as_u32(strings.len()),
        }
        .check(r.kind, r.flags)?;
        next = end;
    }
    if next != args.len() {
        return Err(ProtocolError::CountMismatch(
            command,
            "args",
            args.len() as u64,
            next as u64,
        ));
    }
    Ok(())
}

/// Reads the strings and terms of a checked record's args.
struct Reader<'r, 'a> {
    command: u16,
    text: usize,
    run: &'r [u32],
    strings: &'r StringList<'a>,
}

impl<'a> Reader<'_, 'a> {
    fn missing(&self) -> ProtocolError {
        ProtocolError::CountMismatch(self.command, "change args", self.run.len() as u64, 0)
    }

    fn raw(&self, i: usize) -> Result<u32, ProtocolError> {
        self.run.get(i).copied().ok_or_else(|| self.missing())
    }

    fn text(&self, i: usize) -> Result<(usize, &'a str), ProtocolError> {
        let index = self.raw(i)? as usize;
        self.strings
            .get(index)
            .map(|s| (index, s))
            .ok_or_else(|| self.missing())
    }

    fn optional_text(&self, i: usize) -> Result<Option<(usize, &'a str)>, ProtocolError> {
        if self.raw(i)? == ABSENT {
            Ok(None)
        } else {
            self.text(i).map(Some)
        }
    }

    fn term(&self, i: usize) -> Result<oxrdf::Term, ProtocolError> {
        let (at, text) = self.text(i)?;
        parse_term(self.command, self.text, at, text)
    }

    fn subject(&self, i: usize) -> Result<oxrdf::NamedOrBlankNode, ProtocolError> {
        let (at, text) = self.text(i)?;
        parse_subject(self.command, self.text, at, text)
    }

    fn named(&self, i: usize) -> Result<oxrdf::NamedNode, ProtocolError> {
        let (at, text) = self.text(i)?;
        parse_named(self.command, self.text, at, text)
    }

    fn graph(&self, i: usize) -> Result<GraphName, ProtocolError> {
        match self.optional_text(i)? {
            None => parse_graph(self.command, self.text, 0, None),
            Some((at, text)) => parse_graph(self.command, self.text, at, Some(text)),
        }
    }

    fn iri(&self, i: usize) -> Result<oxrdf::NamedNode, ProtocolError> {
        let (at, text) = self.text(i)?;
        parse_iri(self.command, self.text, at, text)
    }

    fn id(&self, i: usize) -> Result<StatementId, ProtocolError> {
        Ok(StatementId::new(
            u64::from(self.raw(i)?) | (u64::from(self.raw(i + 1)?) << 32),
        ))
    }

    fn statement(&self, by_triple: bool) -> Result<StatementRef, ProtocolError> {
        Ok(if by_triple {
            StatementRef::Triple {
                graph: self.graph(0)?,
                subject: self.subject(1)?,
                predicate: self.named(2)?,
                object: self.term(3)?,
            }
        } else {
            StatementRef::Id {
                graph: self.graph(0)?,
                id: self.id(1)?,
            }
        })
    }
}

fn reader<'r, 'a>(
    command: u16,
    text: usize,
    records: &[u8],
    index: usize,
    args: &'r [u32],
    strings: &'r StringList<'a>,
) -> Result<(Record, Reader<'r, 'a>), ProtocolError> {
    let missing = || ProtocolError::CountMismatch(command, "change records", index as u64, 0);
    let r = record_at(records, index).ok_or_else(missing)?;
    let start = r.start as usize;
    let run = args
        .get(start..start.saturating_add(r.len as usize))
        .ok_or_else(missing)?;
    Ok((
        r,
        Reader {
            command,
            text,
            run,
            strings,
        },
    ))
}

/// Record `index` as a change, its terms parsed.
pub(crate) fn decode_change(
    command: u16,
    sections: (usize, usize, usize),
    records: &[u8],
    index: usize,
    args: &[u32],
    strings: &StringList<'_>,
) -> Result<Change, ProtocolError> {
    let (r, a) = reader(command, sections.2, records, index, args, strings)?;
    Ok(match r.kind {
        INSERT => Change::Insert {
            graph: a.graph(0)?,
            subject: a.subject(1)?,
            predicate: a.named(2)?,
            object: a.term(3)?,
        },
        DELETE => Change::Delete {
            graph: a.graph(0)?,
            subject: a.subject(1)?,
            predicate: a.named(2)?,
            object: a.term(3)?,
        },
        INSERT_EDGE => Change::InsertEdge {
            graph: a.graph(0)?,
            subject: a.subject(1)?,
            predicate: a.named(2)?,
            object: a.term(3)?,
            properties: (4..a.run.len())
                .step_by(2)
                .map(|i| Ok((a.named(i)?, a.term(i + 1)?)))
                .collect::<Result<_, ProtocolError>>()?,
        },
        DELETE_STATEMENT => Change::DeleteStatement {
            graph: a.graph(0)?,
            id: a.id(1)?,
        },
        DELETE_PATTERN => Change::DeletePattern {
            graph: a.graph(0)?,
            subject: a.subject(1)?,
            predicate: a.named(2)?,
        },
        SET_PROPERTY | DELETE_PROPERTY => {
            let by_triple = r.flags & BY_TRIPLE != 0;
            let at = if by_triple { 4 } else { 3 };
            let statement = a.statement(by_triple)?;
            let predicate = a.named(at)?;
            if r.kind == SET_PROPERTY {
                Change::SetProperty {
                    statement,
                    predicate,
                    value: a.term(at + 1)?,
                }
            } else {
                let value = match a.optional_text(at + 1)? {
                    None => None,
                    Some(_) => Some(a.term(at + 1)?),
                };
                Change::DeleteProperty {
                    statement,
                    predicate,
                    value,
                }
            }
        }
        _ => {
            Change::Declare(decode_declaration(command, sections, records, index, args, strings)?.0)
        }
    })
}

/// Record `index` as a declaration and its `extra`.
pub(crate) fn decode_declaration(
    command: u16,
    sections: (usize, usize, usize),
    records: &[u8],
    index: usize,
    args: &[u32],
    strings: &StringList<'_>,
) -> Result<(Declaration, u32), ProtocolError> {
    let (r, a) = reader(command, sections.2, records, index, args, strings)?;
    let names = |from: usize, n: usize| -> Result<Vec<String>, ProtocolError> {
        (from..from + n)
            .map(|i| a.text(i).map(|(_, s)| s.to_string()))
            .collect()
    };
    let declaration = match r.kind {
        PREDICATE => {
            let kind = match r.flags & 0b11 {
                0 => PredicateKind::Edge,
                1 => PredicateKind::Property,
                _ => PredicateKind::Either,
            };
            let mut p = PredicateDeclaration::new(a.iri(0)?)
                .with_kind(kind)
                .with_cardinality(if r.flags & SINGLE != 0 {
                    Cardinality::Single
                } else {
                    Cardinality::Multi
                })
                .with_unique(r.flags & UNIQUE != 0)
                .with_count_index(r.flags & COUNT_INDEX != 0);
            if a.optional_text(1)?.is_some() {
                p = p.with_datatype(a.iri(1)?);
            }
            let tokenizers = a.raw(2)? as usize;
            p = p.with_tokenizers(names(3, tokenizers)?);
            let at = 3 + tokenizers;
            let domains = a.raw(at)? as usize;
            p = p.with_domains(
                (at + 1..at + 1 + domains)
                    .map(|i| a.iri(i))
                    .collect::<Result<_, _>>()?,
            );
            let at = at + 1 + domains;
            if r.flags & VECTOR_INDEX != 0 {
                let factory = a.text(at)?.1.to_string();
                let options = a.raw(at + 1)? as usize;
                let pairs = names(at + 2, 2 * options)?;
                let options = pairs
                    .chunks(2)
                    .map(|kv| (kv[0].clone(), kv[1].clone()))
                    .collect();
                p = p.with_vector_index(VectorIndexSpec::new(factory, options));
            }
            Declaration::Predicate(p)
        }
        ENTITY_TYPE => {
            let parent = match a.optional_text(1)? {
                None => None,
                Some(_) => Some(a.iri(1)?),
            };
            Declaration::EntityType(TypeDeclaration::new(
                a.iri(0)?,
                parent,
                r.flags & ABSTRACT != 0,
            ))
        }
        RELATION => Declaration::Relation(RelationDeclaration::new(
            a.iri(0)?,
            (1..a.run.len())
                .map(|i| a.iri(i))
                .collect::<Result<_, _>>()?,
        )),
        other => {
            return Err(ProtocolError::FieldValue(
                command,
                "change kind",
                other.into(),
            ));
        }
    };
    Ok((declaration, r.extra))
}

/// The changeset of `namespace` held by checked records.
pub(crate) fn decode_changeset(
    command: u16,
    sections: (usize, usize, usize),
    namespace: u64,
    records: &[u8],
    args: &[u32],
    strings: &StringList<'_>,
) -> Result<Changeset, ProtocolError> {
    let count = records.len() / CHANGE_RECORD_SIZE;
    (0..count).try_fold(Changeset::new(namespace), |changeset, index| {
        Ok(changeset.with(decode_change(
            command, sections, records, index, args, strings,
        )?))
    })
}
