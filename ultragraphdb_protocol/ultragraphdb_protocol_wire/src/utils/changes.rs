/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Changes and declarations as records: 16 bytes each, `kind` u8, `flags` u8, two zero bytes, `args_start` u32 and
//! `args_len` u32 into an args section of u32s, and `extra` u32. The args of change `i` follow those of change `i − 1`.
//! A term arg is an index into the body's term table; the IRIs, tokenizers and options a declaration names are
//! indices into a list of names.
//!
//! | Kind | Change | Flags | Args |
//! |---|---|---|---|
//! | 1 | Insert | — | terms: graph or [`crate::ABSENT`], subject, predicate, object |
//! | 2 | InsertEdge | — | graph, subject, predicate, object, then a property and its value per pair |
//! | 3 | Delete | — | graph, subject, predicate, object |
//! | 4 | DeleteStatement | — | graph, the statement ID's low and high u32 |
//! | 5 | DeletePattern | — | graph, subject, predicate |
//! | 6 | SetProperty | bit 0: by triple | the statement (graph, ID low, ID high; or graph, subject, predicate, object), predicate, value |
//! | 7 | DeleteProperty | bit 0: by triple | the statement, predicate, value or [`crate::ABSENT`] |
//! | 8 | Declare a predicate | bits 0–1: kind (0 edge, 1 property, 2 either); bit 2: single; bit 3: unique; bit 4: count index; bit 5: vector index | names: predicate, datatype or [`crate::ABSENT`]; the tokenizer count, then names; the domain count, then names; with a vector index its factory, the option count and each option's name and value |
//! | 9 | Declare an entity type | bit 0: abstract | names: IRI, parent or [`crate::ABSENT`] |
//! | 10 | Declare a relation | — | names: IRI, then its roles |
//!
//! `extra` is zero but in the data dictionary's answer, where an entity type's holds its preorder number.

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::declared::Declared;
use crate::types::declared_entity_type::DeclaredEntityType;
use crate::types::declared_kind::DeclaredKind;
use crate::types::declared_predicate::DeclaredPredicate;
use crate::types::declared_relation::DeclaredRelation;
use crate::types::statement_target::StatementTarget;
use crate::types::string_list::StringList;
use crate::types::write_change::WriteChange;
use crate::utils::constants::ABSENT;
use crate::utils::sections::{as_u32, records as whole_records};
use crate::utils::string_writer::StringWriter;

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

/// Records being written, with their args and names.
#[derive(Debug, Clone, Default)]
pub(crate) struct ChangeWriter {
    pub(crate) records: Vec<u8>,
    pub(crate) args: Vec<u32>,
    pub(crate) names: StringWriter,
}

/// A graph's arg: its term index, [`ABSENT`] for the default graph.
fn graph(g: Option<u32>) -> u32 {
    g.unwrap_or(ABSENT)
}

/// The args naming `statement`, and the flag of a statement named by its triple.
fn statement(statement: &StatementTarget) -> (Vec<u32>, u8) {
    match *statement {
        StatementTarget::Id { graph: g, id } => (vec![graph(g), id as u32, (id >> 32) as u32], 0),
        StatementTarget::Triple {
            graph: g,
            subject,
            predicate,
            object,
        } => (vec![graph(g), subject, predicate, object], BY_TRIPLE),
    }
}

impl ChangeWriter {
    fn record(&mut self, kind: u8, flags: u8, args: Vec<u32>, extra: u32) {
        let start = as_u32(self.args.len());
        let len = as_u32(args.len());
        self.args.extend(args);
        self.records.extend_from_slice(&[kind, flags, 0, 0]);
        self.records.extend_from_slice(&start.to_le_bytes());
        self.records.extend_from_slice(&len.to_le_bytes());
        self.records.extend_from_slice(&extra.to_le_bytes());
    }

    /// Appends `change`'s record.
    pub(crate) fn change(&mut self, change: &WriteChange) {
        match change {
            WriteChange::Insert {
                graph: g,
                subject,
                predicate,
                object,
            } => self.record(INSERT, 0, vec![graph(*g), *subject, *predicate, *object], 0),
            WriteChange::Delete {
                graph: g,
                subject,
                predicate,
                object,
            } => self.record(DELETE, 0, vec![graph(*g), *subject, *predicate, *object], 0),
            WriteChange::InsertEdge {
                graph: g,
                subject,
                predicate,
                object,
                properties,
            } => {
                let mut args = vec![graph(*g), *subject, *predicate, *object];
                properties.iter().for_each(|(p, v)| args.extend([*p, *v]));
                self.record(INSERT_EDGE, 0, args, 0);
            }
            WriteChange::DeleteStatement { graph: g, id } => {
                self.record(
                    DELETE_STATEMENT,
                    0,
                    vec![graph(*g), *id as u32, (*id >> 32) as u32],
                    0,
                );
            }
            WriteChange::DeletePattern {
                graph: g,
                subject,
                predicate,
            } => self.record(DELETE_PATTERN, 0, vec![graph(*g), *subject, *predicate], 0),
            WriteChange::SetProperty {
                statement: target,
                predicate,
                value,
            } => {
                let (mut args, flags) = statement(target);
                args.extend([*predicate, *value]);
                self.record(SET_PROPERTY, flags, args, 0);
            }
            WriteChange::DeleteProperty {
                statement: target,
                predicate,
                value,
            } => {
                let (mut args, flags) = statement(target);
                args.extend([*predicate, value.unwrap_or(ABSENT)]);
                self.record(DELETE_PROPERTY, flags, args, 0);
            }
            WriteChange::Declare(declaration) => self.declaration(declaration, 0),
        }
    }

    /// Appends `declaration`'s record with `extra`.
    pub(crate) fn declaration(&mut self, declaration: &Declared, extra: u32) {
        match declaration {
            Declared::Predicate(p) => {
                let kind = match p.kind() {
                    DeclaredKind::Edge => 0,
                    DeclaredKind::Property => 1,
                    DeclaredKind::Either => 2,
                };
                let flags = kind
                    | if p.single() { SINGLE } else { 0 }
                    | if p.unique() { UNIQUE } else { 0 }
                    | if p.count_index() { COUNT_INDEX } else { 0 }
                    | if p.vector_index().is_some() {
                        VECTOR_INDEX
                    } else {
                        0
                    };
                let mut args = vec![self.names.intern(p.predicate())];
                let datatype = p.datatype().map_or(ABSENT, |d| self.names.intern(d));
                args.push(datatype);
                args.push(as_u32(p.tokenizers().len()));
                for tokenizer in p.tokenizers() {
                    args.push(self.names.intern(tokenizer));
                }
                args.push(as_u32(p.domains().len()));
                for domain in p.domains() {
                    args.push(self.names.intern(domain));
                }
                if let Some((factory, options)) = p.vector_index() {
                    args.push(self.names.intern(factory));
                    args.push(as_u32(options.len()));
                    for (name, value) in options {
                        args.push(self.names.intern(name));
                        args.push(self.names.intern(value));
                    }
                }
                self.record(PREDICATE, flags, args, extra);
            }
            Declared::EntityType(t) => {
                let mut args = vec![self.names.intern(t.iri())];
                let parent = t.parent().map_or(ABSENT, |p| self.names.intern(p));
                args.push(parent);
                self.record(
                    ENTITY_TYPE,
                    if t.is_abstract() { ABSTRACT } else { 0 },
                    args,
                    extra,
                );
            }
            Declared::Relation(r) => {
                let mut args = vec![self.names.intern(r.iri())];
                for role in r.roles() {
                    args.push(self.names.intern(role));
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
    let r = records.get(
        index.checked_mul(CHANGE_RECORD_SIZE)?
            ..index.checked_add(1)?.checked_mul(CHANGE_RECORD_SIZE)?,
    )?;
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
    terms: u32,
    names: u32,
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

    fn below(&self, i: usize, limit: u32) -> Result<(), ProtocolError> {
        let value = self.run.get(i).copied().unwrap_or(ABSENT);
        if value < limit {
            Ok(())
        } else {
            Err(ProtocolError::IndexOutOfRange(
                self.command,
                self.section,
                self.base + i,
                value,
                limit,
            ))
        }
    }

    /// Arg `i` names a term.
    fn term(&self, i: usize) -> Result<(), ProtocolError> {
        self.below(i, self.terms)
    }

    /// Arg `i` names a term or is absent.
    fn optional_term(&self, i: usize) -> Result<(), ProtocolError> {
        if self.run.get(i) == Some(&ABSENT) {
            Ok(())
        } else {
            self.term(i)
        }
    }

    fn terms(&self, range: std::ops::Range<usize>) -> Result<(), ProtocolError> {
        range.into_iter().try_for_each(|i| self.term(i))
    }

    /// Arg `i` names a name.
    fn name(&self, i: usize) -> Result<(), ProtocolError> {
        self.below(i, self.names)
    }

    fn optional_name(&self, i: usize) -> Result<(), ProtocolError> {
        if self.run.get(i) == Some(&ABSENT) {
            Ok(())
        } else {
            self.name(i)
        }
    }

    /// A count at arg `at` followed by that many `per`-name groups; the arg after them.
    fn counted(&self, at: usize, per: usize) -> Result<usize, ProtocolError> {
        let n = *self.run.get(at).ok_or_else(|| self.mismatch(at + 1))? as usize;
        let end = (at + 1).saturating_add(n.saturating_mul(per));
        if end > self.run.len() {
            return Err(self.mismatch(end));
        }
        (at + 1..end).try_for_each(|i| self.name(i))?;
        Ok(end)
    }

    fn check(&self, kind: u8, flags: u8) -> Result<(), ProtocolError> {
        let len = self.run.len();
        match kind {
            INSERT | DELETE => {
                self.count(4)?;
                self.optional_term(0)?;
                self.terms(1..4)
            }
            INSERT_EDGE => {
                if len < 4 || !(len - 4).is_multiple_of(2) {
                    return Err(self.mismatch(if len < 4 { 4 } else { len + 1 }));
                }
                self.optional_term(0)?;
                self.terms(1..len)
            }
            DELETE_STATEMENT => {
                self.count(3)?;
                self.optional_term(0)
            }
            DELETE_PATTERN => {
                self.count(3)?;
                self.optional_term(0)?;
                self.terms(1..3)
            }
            SET_PROPERTY | DELETE_PROPERTY => {
                let by_triple = flags & BY_TRIPLE != 0;
                let at = if by_triple { 4 } else { 3 };
                self.count(at + 2)?;
                self.optional_term(0)?;
                if by_triple {
                    self.terms(1..4)?;
                }
                self.term(at)?;
                if kind == SET_PROPERTY {
                    self.term(at + 1)
                } else {
                    self.optional_term(at + 1)
                }
            }
            PREDICATE => {
                if len < 3 {
                    return Err(self.mismatch(3));
                }
                self.name(0)?;
                self.optional_name(1)?;
                let after_tokenizers = self.counted(2, 1)?;
                let mut at = self.counted(after_tokenizers, 1)?;
                if flags & VECTOR_INDEX != 0 {
                    if at + 2 > len {
                        return Err(self.mismatch(at + 2));
                    }
                    self.name(at)?;
                    at = self.counted(at + 1, 2)?;
                }
                self.count(at)
            }
            ENTITY_TYPE => {
                self.count(2)?;
                self.name(0)?;
                self.optional_name(1)
            }
            _ => {
                if len == 0 {
                    return Err(self.mismatch(1));
                }
                (0..len).try_for_each(|i| self.name(i))
            }
        }
    }
}

/// The records of a body checked: whole records, kinds known (only declarations when `declarations_only`, in the
/// order predicates, entity types, relations), flags and zero bytes as the kind defines, args contiguous and consumed
/// exactly, each term index below `terms` and each name index in `names`. `sections` are the records' and the args'.
pub(crate) fn check_records(
    command: u16,
    sections: (usize, usize),
    records: &[u8],
    args: &[u32],
    terms: usize,
    names: &StringList<'_>,
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
            terms: as_u32(terms),
            names: as_u32(names.len()),
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

/// The args of checked record `index`, and the record.
fn run<'r>(records: &[u8], index: usize, args: &'r [u32]) -> Option<(Record, &'r [u32])> {
    let r = record_at(records, index)?;
    let start = r.start as usize;
    Some((r, args.get(start..start.checked_add(r.len as usize)?)?))
}

/// Checked record `index` as a change; `None` past the last record.
pub(crate) fn decode_change(
    records: &[u8],
    index: usize,
    args: &[u32],
    names: &StringList<'_>,
) -> Option<WriteChange> {
    let (r, a) = run(records, index, args)?;
    let arg = |i: usize| a.get(i).copied();
    let graph = || arg(0).map(|g| (g != ABSENT).then_some(g));
    let id = |i: usize| Some(u64::from(arg(i)?) | (u64::from(arg(i + 1)?) << 32));
    Some(match r.kind {
        INSERT => WriteChange::Insert {
            graph: graph()?,
            subject: arg(1)?,
            predicate: arg(2)?,
            object: arg(3)?,
        },
        DELETE => WriteChange::Delete {
            graph: graph()?,
            subject: arg(1)?,
            predicate: arg(2)?,
            object: arg(3)?,
        },
        INSERT_EDGE => WriteChange::InsertEdge {
            graph: graph()?,
            subject: arg(1)?,
            predicate: arg(2)?,
            object: arg(3)?,
            properties: a
                .get(4..)?
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| (pair[0], pair[1]))
                .collect(),
        },
        DELETE_STATEMENT => WriteChange::DeleteStatement {
            graph: graph()?,
            id: id(1)?,
        },
        DELETE_PATTERN => WriteChange::DeletePattern {
            graph: graph()?,
            subject: arg(1)?,
            predicate: arg(2)?,
        },
        SET_PROPERTY | DELETE_PROPERTY => {
            let by_triple = r.flags & BY_TRIPLE != 0;
            let target = if by_triple {
                StatementTarget::Triple {
                    graph: graph()?,
                    subject: arg(1)?,
                    predicate: arg(2)?,
                    object: arg(3)?,
                }
            } else {
                StatementTarget::Id {
                    graph: graph()?,
                    id: id(1)?,
                }
            };
            let at = if by_triple { 4 } else { 3 };
            if r.kind == SET_PROPERTY {
                WriteChange::SetProperty {
                    statement: target,
                    predicate: arg(at)?,
                    value: arg(at + 1)?,
                }
            } else {
                WriteChange::DeleteProperty {
                    statement: target,
                    predicate: arg(at)?,
                    value: arg(at + 1).filter(|v| *v != ABSENT),
                }
            }
        }
        _ => WriteChange::Declare(decode_declaration(records, index, args, names)?.0),
    })
}

/// Checked record `index` as a declaration and its `extra`; `None` past the last record or for a change.
pub(crate) fn decode_declaration(
    records: &[u8],
    index: usize,
    args: &[u32],
    names: &StringList<'_>,
) -> Option<(Declared, u32)> {
    let (r, a) = run(records, index, args)?;
    let name = |i: usize| names.get(*a.get(i)? as usize).map(str::to_string);
    let optional = |i: usize| {
        if a.get(i) == Some(&ABSENT) {
            Some(None)
        } else {
            name(i).map(Some)
        }
    };
    let list = |from: usize, n: usize| (from..from + n).map(name).collect::<Option<Vec<String>>>();
    let declaration = match r.kind {
        PREDICATE => {
            let kind = match r.flags & 0b11 {
                0 => DeclaredKind::Edge,
                1 => DeclaredKind::Property,
                _ => DeclaredKind::Either,
            };
            let mut p = DeclaredPredicate::new(name(0)?)
                .with_kind(kind)
                .with_single(r.flags & SINGLE != 0)
                .with_unique(r.flags & UNIQUE != 0)
                .with_count_index(r.flags & COUNT_INDEX != 0);
            if let Some(datatype) = optional(1)? {
                p = p.with_datatype(datatype);
            }
            let tokenizers = *a.get(2)? as usize;
            p = p.with_tokenizers(list(3, tokenizers)?);
            let at = 3 + tokenizers;
            let domains = *a.get(at)? as usize;
            p = p.with_domains(list(at + 1, domains)?);
            let at = at + 1 + domains;
            if r.flags & VECTOR_INDEX != 0 {
                let factory = name(at)?;
                let options = *a.get(at + 1)? as usize;
                let pairs = list(at + 2, 2 * options)?;
                let options = pairs
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|kv| (kv[0].clone(), kv[1].clone()))
                    .collect();
                p = p.with_vector_index(factory, options);
            }
            Declared::Predicate(p)
        }
        ENTITY_TYPE => Declared::EntityType(DeclaredEntityType::new(
            name(0)?,
            optional(1)?,
            r.flags & ABSTRACT != 0,
        )),
        RELATION => Declared::Relation(DeclaredRelation::new(
            name(0)?,
            list(1, a.len().checked_sub(1)?)?,
        )),
        _ => return None,
    };
    Some((declaration, r.extra))
}
