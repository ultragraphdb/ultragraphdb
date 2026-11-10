/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::str::FromStr;

use oxilangtag::LanguageTag;
use oxrdf::NamedNodeRef;
use oxsdatatypes::{
    Boolean, Date, DateTime, DayTimeDuration, Double, Duration, Float, GDay, GMonth, GMonthDay,
    GYear, GYearMonth, Time, YearMonthDuration,
};
use ultragraphdb_core_terms::{KnownDatatype, parse_vector};
use ultragraphdb_protocol_wire::{
    CommitOperation, ErrorCode, ErrorReply, Link, PatternSlot, QuadPattern, RequestTermView,
    StatementTarget, TermTableView, WriteChange,
};

use crate::utils::iri_place::{IriPlace, iri_place};

/// A term breaking a rule of `PROTOCOL.md` §4.4: code 7 on the request's stream, `detail` its index.
fn refused(index: u32, rule: impl Into<String>) -> ErrorReply {
    ErrorReply::new(ErrorCode::Refused, rule).with_detail(index.into())
}

/// Checks each entry of `table` against the server's rules of its kind: an absolute IRI, a well-formed language tag, a lexical
/// form valid for a known datatype.
///
/// # Errors
///
/// Code 7 naming the first term that breaks one.
pub fn check_table(table: &TermTableView<'_>) -> Result<(), ErrorReply> {
    for index in 0..u32::try_from(table.len()).unwrap_or(u32::MAX) {
        match table.get(index) {
            Some(RequestTermView::Iri(iri)) => check_iri(index, iri)?,
            Some(RequestTermView::Literal {
                lexical, datatype, ..
            }) => check_literal(index, lexical, datatype)?,
            Some(RequestTermView::LanguageLiteral { language, .. }) => {
                if LanguageTag::parse(language).is_err() {
                    return Err(refused(
                        index,
                        format!("the language tag {language} is not well formed"),
                    ));
                }
            }
            Some(RequestTermView::BlankNode(_) | RequestTermView::Triple { .. }) | None => {}
        }
    }
    Ok(())
}

fn check_iri(index: u32, iri: &str) -> Result<(), ErrorReply> {
    if NamedNodeRef::new(iri).is_err() {
        return Err(refused(index, format!("{iri} is not an absolute IRI")));
    }
    Ok(())
}

fn check_literal(index: u32, lexical: &str, datatype: Option<&str>) -> Result<(), ErrorReply> {
    let Some(datatype) = datatype else {
        return Ok(());
    };
    check_iri(index, datatype)?;
    if KnownDatatype::from_iri(datatype).is_none_or(|known| valid_lexical(known, lexical)) {
        return Ok(());
    }
    Err(refused(
        index,
        format!("{lexical} is no lexical form of {datatype}"),
    ))
}

fn parses<T: FromStr>(lexical: &str) -> bool {
    lexical.parse::<T>().is_ok()
}

/// Whether `lexical` is in `xsd:integer`'s lexical space, `[+-]?[0-9]+`, whatever its size: an integer no payload holds is kept
/// by its lexical form (`PROTOCOL.md` §10.2).
fn is_integer(lexical: &str) -> bool {
    let digits = lexical.strip_prefix(['+', '-']).unwrap_or(lexical);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether `lexical` is in `xsd:decimal`'s lexical space, `[+-]?([0-9]+(\\.[0-9]*)?|\\.[0-9]+)`, whatever its size and fraction: a
/// decimal no payload holds is kept by its lexical form (`PROTOCOL.md` §10.2).
fn is_decimal(lexical: &str) -> bool {
    let unsigned = lexical.strip_prefix(['+', '-']).unwrap_or(lexical);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    digits(whole) && digits(fraction) && !(whole.is_empty() && fraction.is_empty())
}

/// Whether `lexical` is valid for `known`. A datatype the XSD parser has no type for, a tokenizer's own, takes any form.
fn valid_lexical(known: KnownDatatype, lexical: &str) -> bool {
    match known {
        KnownDatatype::Boolean => parses::<Boolean>(lexical),
        KnownDatatype::Decimal => is_decimal(lexical),
        KnownDatatype::Double => parses::<Double>(lexical),
        KnownDatatype::Float => parses::<Float>(lexical),
        KnownDatatype::DateTime => parses::<DateTime>(lexical),
        KnownDatatype::Time => parses::<Time>(lexical),
        KnownDatatype::Date => parses::<Date>(lexical),
        KnownDatatype::GYearMonth => parses::<GYearMonth>(lexical),
        KnownDatatype::GYear => parses::<GYear>(lexical),
        KnownDatatype::GMonthDay => parses::<GMonthDay>(lexical),
        KnownDatatype::GDay => parses::<GDay>(lexical),
        KnownDatatype::GMonth => parses::<GMonth>(lexical),
        KnownDatatype::Duration => parses::<Duration>(lexical),
        KnownDatatype::YearMonthDuration => parses::<YearMonthDuration>(lexical),
        KnownDatatype::DayTimeDuration => parses::<DayTimeDuration>(lexical),
        KnownDatatype::DateTimeStamp => parses::<DateTime>(lexical),
        KnownDatatype::Integer
        | KnownDatatype::Long
        | KnownDatatype::Int
        | KnownDatatype::Short
        | KnownDatatype::Byte
        | KnownDatatype::NonNegativeInteger
        | KnownDatatype::PositiveInteger
        | KnownDatatype::NonPositiveInteger
        | KnownDatatype::NegativeInteger
        | KnownDatatype::UnsignedLong
        | KnownDatatype::UnsignedInt
        | KnownDatatype::UnsignedShort
        | KnownDatatype::UnsignedByte => is_integer(lexical),
        KnownDatatype::Float32Vector => parse_vector(lexical).is_ok(),
        KnownDatatype::GeoJsonLiteral | KnownDatatype::WktLiteral | KnownDatatype::AnyUri => true,
    }
}

/// Whether a read of `pattern` binds an IRI nobody can hold: the frontend then answers it itself, with no row.
#[must_use]
pub fn binds_nobody(table: &TermTableView<'_>, pattern: &QuadPattern, tenant: u64) -> bool {
    pattern.slots().into_iter().any(|slot| match slot {
        PatternSlot::Bound(index) => matches!(
            table.get(index),
            Some(RequestTermView::Iri(iri)) if iri_place(tenant, iri) == IriPlace::Nobody
        ),
        PatternSlot::Unbound | PatternSlot::DefaultGraph | PatternSlot::Parameter(_) => false,
    })
}

/// Checks a pattern's slots: its graph and subject take an IRI, its predicate an IRI, its object no blank node's label and no
/// triple term.
///
/// # Errors
///
/// Code 7 naming the term and the rule.
pub fn check_pattern(table: &TermTableView<'_>, pattern: &QuadPattern) -> Result<(), ErrorReply> {
    let iri_slot = |slot: PatternSlot, name: &str| -> Result<(), ErrorReply> {
        if let PatternSlot::Bound(index) = slot
            && !matches!(table.get(index), Some(RequestTermView::Iri(_)))
        {
            return Err(refused(
                index,
                format!("a pattern's {name} slot takes an IRI"),
            ));
        }
        Ok(())
    };
    iri_slot(pattern.graph(), "graph")?;
    iri_slot(pattern.subject(), "subject")?;
    iri_slot(pattern.predicate(), "predicate")?;
    if let PatternSlot::Bound(index) = pattern.object() {
        match table.get(index) {
            Some(RequestTermView::BlankNode(_)) => {
                return Err(refused(
                    index,
                    "a pattern's object slot takes no blank node's label, which a request names by its skolem IRI",
                ));
            }
            Some(RequestTermView::Triple { .. }) => {
                return Err(refused(
                    index,
                    "a pattern's object slot takes no triple term",
                ));
            }
            Some(
                RequestTermView::Iri(_)
                | RequestTermView::Literal { .. }
                | RequestTermView::LanguageLiteral { .. },
            )
            | None => {}
        }
    }
    Ok(())
}

/// The rules of a change's or an operation's slots, in `tenant`.
struct Slots<'t, 'a> {
    table: &'t TermTableView<'a>,
    tenant: u64,
}

impl Slots<'_, '_> {
    /// A graph or a subject: an IRI or a blank node, and no IRI nobody can hold.
    fn node(&self, index: u32, name: &str) -> Result<(), ErrorReply> {
        match self.table.get(index) {
            Some(RequestTermView::BlankNode(_)) => Ok(()),
            Some(RequestTermView::Iri(_)) => self.holdable(index),
            Some(
                RequestTermView::Literal { .. }
                | RequestTermView::LanguageLiteral { .. }
                | RequestTermView::Triple { .. },
            )
            | None => Err(refused(
                index,
                format!("a change's {name} takes an IRI or a blank node"),
            )),
        }
    }

    fn graph(&self, index: Option<u32>) -> Result<(), ErrorReply> {
        index.map_or(Ok(()), |index| self.node(index, "graph"))
    }

    fn predicate(&self, index: u32) -> Result<(), ErrorReply> {
        if matches!(self.table.get(index), Some(RequestTermView::Iri(_))) {
            return self.holdable(index);
        }
        Err(refused(index, "a change's predicate takes an IRI"))
    }

    /// Any slot: no triple term, which no statement holds, and no IRI nobody can hold.
    fn value(&self, index: u32) -> Result<(), ErrorReply> {
        if matches!(self.table.get(index), Some(RequestTermView::Triple { .. })) {
            return Err(refused(
                index,
                "a change takes no triple term: no statement holds one",
            ));
        }
        self.holdable(index)
    }

    fn holdable(&self, index: u32) -> Result<(), ErrorReply> {
        if let Some(RequestTermView::Iri(iri)) = self.table.get(index) {
            return match iri_place(self.tenant, iri) {
                IriPlace::Nobody => Err(refused(
                    index,
                    format!("nobody can hold the skolem IRI {iri}"),
                )),
                IriPlace::TooLong => Err(refused(index, "an IRI past the longest a key holds")),
                IriPlace::Ordinary(_) | IriPlace::Skolem(_) => Ok(()),
            };
        }
        Ok(())
    }

    fn statement(&self, target: &StatementTarget) -> Result<(), ErrorReply> {
        match target {
            StatementTarget::Reifier { graph, reifier } => {
                self.graph(*graph)?;
                self.node(*reifier, "reifier")
            }
            StatementTarget::Triple {
                graph,
                subject,
                predicate,
                object,
            } => {
                self.graph(*graph)?;
                self.node(*subject, "subject")?;
                self.predicate(*predicate)?;
                self.value(*object)
            }
        }
    }

    fn properties(&self, properties: &[(u32, u32)]) -> Result<(), ErrorReply> {
        properties.iter().try_for_each(|(key, value)| {
            self.predicate(*key)?;
            self.value(*value)
        })
    }
}

/// Checks `change`'s slots in `tenant`.
///
/// # Errors
///
/// Code 7 naming the term and the rule.
pub fn check_change(
    table: &TermTableView<'_>,
    tenant: u64,
    change: &WriteChange,
) -> Result<(), ErrorReply> {
    let slots = Slots { table, tenant };
    match change {
        WriteChange::Insert {
            graph,
            subject,
            predicate,
            object,
        }
        | WriteChange::Delete {
            graph,
            subject,
            predicate,
            object,
        } => {
            slots.graph(*graph)?;
            slots.node(*subject, "subject")?;
            slots.predicate(*predicate)?;
            slots.value(*object)
        }
        WriteChange::InsertEdge {
            graph,
            subject,
            predicate,
            object,
            properties,
        } => {
            slots.graph(*graph)?;
            slots.node(*subject, "subject")?;
            slots.predicate(*predicate)?;
            slots.value(*object)?;
            slots.properties(properties)
        }
        WriteChange::DeleteStatement { graph, reifier } => {
            slots.graph(*graph)?;
            slots.node(*reifier, "reifier")
        }
        WriteChange::DeletePattern {
            graph,
            subject,
            predicate,
        } => {
            slots.graph(*graph)?;
            slots.node(*subject, "subject")?;
            slots.predicate(*predicate)
        }
        WriteChange::SetProperty {
            statement,
            predicate,
            value,
        } => {
            slots.statement(statement)?;
            slots.predicate(*predicate)?;
            slots.value(*value)
        }
        WriteChange::DeleteProperty {
            statement,
            predicate,
            value,
        } => {
            slots.statement(statement)?;
            slots.predicate(*predicate)?;
            value.map_or(Ok(()), |value| slots.value(value))
        }
        WriteChange::Declare(_) => Ok(()),
    }
}

/// Checks `operation`'s slots in `tenant`.
///
/// # Errors
///
/// Code 7 naming the term and the rule.
pub fn check_operation(
    table: &TermTableView<'_>,
    tenant: u64,
    operation: &CommitOperation,
) -> Result<(), ErrorReply> {
    let slots = Slots { table, tenant };
    match operation {
        CommitOperation::InsertIfAbsent {
            graph,
            predicate,
            value,
            properties,
        } => {
            slots.graph(*graph)?;
            slots.predicate(*predicate)?;
            slots.value(*value)?;
            slots.properties(properties)
        }
        CommitOperation::UpsertAndLink {
            graph,
            predicate,
            value,
            properties,
            links,
        } => {
            slots.graph(*graph)?;
            slots.predicate(*predicate)?;
            slots.value(*value)?;
            slots.properties(properties)?;
            links.iter().try_for_each(|link: &Link| {
                slots.predicate(link.predicate())?;
                slots.node(link.other(), "link")
            })
        }
        CommitOperation::CompareAndSet {
            graph,
            subject,
            predicate,
            expected,
            new,
        } => {
            slots.graph(*graph)?;
            slots.node(*subject, "subject")?;
            slots.predicate(*predicate)?;
            slots.value(*expected)?;
            slots.value(*new)
        }
        CommitOperation::CounterAdd {
            graph,
            subject,
            predicate,
            ..
        } => {
            slots.graph(*graph)?;
            slots.node(*subject, "subject")?;
            slots.predicate(*predicate)
        }
        // `PROTOCOL.md` §7.3: either kind in a tenant with no schema is refused with code 7, `detail` 0; no tenant holds
        // one until Install is served, and the commit step decides neither kind before then.
        CommitOperation::DeleteInstance { .. } => Err(instance_operation(5, "delete an instance")),
        CommitOperation::RetractInstance { .. } => {
            Err(instance_operation(6, "retract an instance"))
        }
    }
}

/// The refusal of an Operation of `kind`, which names an instance of a declared type, in a tenant that holds no schema:
/// code 7, `detail` 0, the message naming the kind.
fn instance_operation(kind: u8, name: &str) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::Refused,
        format!("an Operation of kind {kind}, {name}, is refused: the tenant holds no schema"),
    )
}
