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
//! | 4 | DeleteStatement | — | graph, the reifier's skolem IRI |
//! | 5 | DeletePattern | — | graph, subject, predicate |
//! | 6 | SetProperty | bit 0: by quad | the statement (graph, reifier; or graph, subject, predicate, object), predicate, value |
//! | 7 | DeleteProperty | bit 0: by quad | the statement, predicate, value or [`crate::ABSENT`] |
//! | 8 | Declare a predicate | bits 0–1: kind (0 edge, 1 property, 2 either); bit 2: single; bit 3: unique; bit 4: count index; bit 5: vector index | names: predicate, datatype or [`crate::ABSENT`]; the tokenizer count, then names; the domain count, then names; with a vector index its factory, the option count and each option's name and value |
//! | 9 | Declare an entity type | bit 0: abstract | names: IRI, parent or [`crate::ABSENT`] |
//! | 10 | Declare a relation | — | names: IRI, then its roles |
//!
//! `extra` is zero in every record, a Describe's declarations included: a record names no place in the type hierarchy
//! (`PROTOCOL.md` §4.6, §8.3).

use ultragraphdb_core_body::{StringList, StringWriter, as_u32};
use ultragraphdb_error_protocol::ProtocolError;

use crate::types::declared::Declared;
use crate::types::declared_entity_type::DeclaredEntityType;
use crate::types::declared_kind::DeclaredKind;
use crate::types::declared_predicate::DeclaredPredicate;
use crate::types::declared_relation::DeclaredRelation;
use crate::types::statement_target::StatementTarget;
use crate::types::write_change::WriteChange;
use crate::utils::body::records as whole_records;
use crate::utils::constants::ABSENT;

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

/// Which record kinds a body takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecordKinds {
    /// Kinds 1 to 10: a Write.
    Every,
    /// Kinds 8 to 10, in the order predicates, entity types, relations, `extra` a Describe's: a Describe.
    Declarations,
    /// Kinds 1 to 7; a declaration refused as [`ProtocolError::field_value`] `"record"`: a DirectWrite or a
    /// ConditionalWrite.
    Changes,
}

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
        StatementTarget::Reifier { graph: g, reifier } => (vec![graph(g), reifier], 0),
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
            WriteChange::DeleteStatement { graph: g, reifier } => {
                self.record(DELETE_STATEMENT, 0, vec![graph(*g), *reifier], 0);
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
        r.get(at..)
            .and_then(<[u8]>::first_chunk::<4>)
            .map_or(0, |b| u32::from_le_bytes(*b))
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
        ProtocolError::count_mismatch(
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
            Err(ProtocolError::index_out_of_range(
                self.command,
                self.section,
                self.base.saturating_add(i),
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
        let first = at.checked_add(1).ok_or_else(|| self.mismatch(at))?;
        let n = *self.run.get(at).ok_or_else(|| self.mismatch(first))? as usize;
        let Some(end) = n
            .checked_mul(per)
            .and_then(|groups| first.checked_add(groups))
            .filter(|end| *end <= self.run.len())
        else {
            // The count the args would need, which a refusal only names.
            return Err(self.mismatch(first.saturating_add(n.saturating_mul(per))));
        };
        (first..end).try_for_each(|i| self.name(i))?;
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
                match len.checked_sub(4) {
                    None => return Err(self.mismatch(4)),
                    Some(edges) if !edges.is_multiple_of(2) => {
                        return Err(self.mismatch(len.saturating_add(1)));
                    }
                    Some(_) => {}
                }
                self.optional_term(0)?;
                self.terms(1..len)
            }
            DELETE_STATEMENT => {
                self.count(2)?;
                self.optional_term(0)?;
                self.term(1)
            }
            DELETE_PATTERN => {
                self.count(3)?;
                self.optional_term(0)?;
                self.terms(1..3)
            }
            SET_PROPERTY | DELETE_PROPERTY => {
                let by_triple = flags & BY_TRIPLE != 0;
                // The predicate's arg, the value's after it, and the count of args.
                let (at, value, count) = if by_triple { (4, 5, 6) } else { (2, 3, 4) };
                self.count(count)?;
                self.optional_term(0)?;
                self.terms(1..at)?;
                self.term(at)?;
                if kind == SET_PROPERTY {
                    self.term(value)
                } else {
                    self.optional_term(value)
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
                    let Some(options) = at.checked_add(1).filter(|options| *options < len) else {
                        return Err(self.mismatch(at.saturating_add(2)));
                    };
                    self.name(at)?;
                    at = self.counted(options, 2)?;
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

impl Record {
    /// Refused unless the kind is known and `kinds` takes it (a Describe's none before `last_kind`), and the flags,
    /// zero bytes and `extra` are as the kind defines.
    fn check_fields(
        self,
        command: u16,
        kinds: RecordKinds,
        last_kind: u8,
    ) -> Result<(), ProtocolError> {
        let declarations_only = kinds == RecordKinds::Declarations;
        let known = if declarations_only {
            (PREDICATE..=RELATION).contains(&self.kind)
        } else {
            (INSERT..=RELATION).contains(&self.kind)
        };
        if !known {
            return Err(ProtocolError::field_value(
                command,
                "change kind",
                self.kind.into(),
            ));
        }
        if kinds == RecordKinds::Changes && self.kind >= PREDICATE {
            return Err(ProtocolError::field_value(
                command,
                "record",
                self.kind.into(),
            ));
        }
        if declarations_only && self.kind < last_kind {
            return Err(ProtocolError::field_value(
                command,
                "declaration order",
                self.kind.into(),
            ));
        }
        let flags_invalid = self.flags & !allowed_flags(self.kind) != 0
            || (self.kind == PREDICATE && self.flags & 0b11 == 0b11);
        if flags_invalid {
            return Err(ProtocolError::field_value(
                command,
                "flags",
                self.flags.into(),
            ));
        }
        if self.zero != 0 {
            return Err(ProtocolError::field_value(
                command,
                "change record",
                self.zero.into(),
            ));
        }
        if self.extra != 0 {
            return Err(ProtocolError::field_value(
                command,
                "extra",
                self.extra.into(),
            ));
        }
        Ok(())
    }
}

/// The records of a body checked: whole records, each of the kinds `kinds` takes, flags and zero bytes as the kind
/// defines, args contiguous and consumed exactly, each term index below `terms` and each name index in `names`.
/// `sections` are the records' and the args'.
pub(crate) fn check_records(
    command: u16,
    sections: (usize, usize),
    records: &[u8],
    args: &[u32],
    terms: usize,
    names: &StringList<'_>,
    kinds: RecordKinds,
) -> Result<(), ProtocolError> {
    let next = consume_records(command, sections, records, args, terms, names, kinds)?;
    if next != args.len() {
        return Err(ProtocolError::count_mismatch(
            command,
            "args",
            // The casts are lossless: a `usize` is at most 64 bits.
            args.len() as u64,
            next as u64,
        ));
    }
    Ok(())
}

/// The records of a body checked as [`check_records`] checks them, any args after theirs left to the caller; the number
/// of args they consumed.
pub(crate) fn consume_records(
    command: u16,
    sections: (usize, usize),
    records: &[u8],
    args: &[u32],
    terms: usize,
    names: &StringList<'_>,
    kinds: RecordKinds,
) -> Result<usize, ProtocolError> {
    let count = whole_records(command, sections.0, records, CHANGE_RECORD_SIZE)?.len();
    let mut next = 0usize;
    let mut last_kind = 0u8;
    for index in 0..count {
        let Some(r) = record_at(records, index) else {
            break;
        };
        r.check_fields(command, kinds, last_kind)?;
        last_kind = r.kind;
        if r.start as usize != next {
            return Err(ProtocolError::field_value(
                command,
                "args_start",
                r.start.into(),
            ));
        }
        let end = next.saturating_add(r.len as usize);
        let run = args.get(next..end).ok_or_else(|| {
            // Lossless: a `usize` is at most 64 bits.
            ProtocolError::count_mismatch(command, "args", args.len() as u64, end as u64)
        })?;
        Run {
            command,
            section: sections.1,
            base: next,
            run,
            terms: crate::utils::fields::count(command, "terms", terms)?,
            names: crate::utils::fields::count(command, "names", names.len())?,
        }
        .check(r.kind, r.flags)?;
        next = end;
    }
    Ok(next)
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
            reifier: arg(1)?,
        },
        DELETE_PATTERN => WriteChange::DeletePattern {
            graph: graph()?,
            subject: arg(1)?,
            predicate: arg(2)?,
        },
        SET_PROPERTY | DELETE_PROPERTY => decode_property(r, a)?,
        _ => WriteChange::Declare(decode_declaration(records, index, args, names)?.0),
    })
}

/// Checked record `r`, a property set or delete, over its args `a`.
fn decode_property(r: Record, a: &[u32]) -> Option<WriteChange> {
    let arg = |i: usize| a.get(i).copied();
    let graph = || arg(0).map(|g| (g != ABSENT).then_some(g));
    let by_triple = r.flags & BY_TRIPLE != 0;
    let target = if by_triple {
        StatementTarget::Triple {
            graph: graph()?,
            subject: arg(1)?,
            predicate: arg(2)?,
            object: arg(3)?,
        }
    } else {
        StatementTarget::Reifier {
            graph: graph()?,
            reifier: arg(1)?,
        }
    };
    // The predicate's arg and the value's after it.
    let (at, value) = if by_triple { (4, 5) } else { (2, 3) };
    Some(if r.kind == SET_PROPERTY {
        WriteChange::SetProperty {
            statement: target,
            predicate: arg(at)?,
            value: arg(value)?,
        }
    } else {
        WriteChange::DeleteProperty {
            statement: target,
            predicate: arg(at)?,
            value: arg(value).filter(|v| *v != ABSENT),
        }
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
    let list = |from: usize, n: usize| {
        (from..from.checked_add(n)?)
            .map(name)
            .collect::<Option<Vec<String>>>()
    };
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
            let at = tokenizers.checked_add(3)?;
            let domains = *a.get(at)? as usize;
            p = p.with_domains(list(at.checked_add(1)?, domains)?);
            let at = at.checked_add(1)?.checked_add(domains)?;
            if r.flags & VECTOR_INDEX != 0 {
                let factory = name(at)?;
                let option_count = *a.get(at.checked_add(1)?)? as usize; // lossless: a `usize` is at least 32 bits
                let pairs = list(at.checked_add(2)?, option_count.checked_mul(2)?)?;
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
