/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use oxrdf::{BaseDirection, Literal, NamedNode};
use ultragraphdb_core_ids::{SystemId, TypePrefix};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_terms::{Datatype, EncodedTerm, Hash128, TermError, TermView, Text};
use ultragraphdb_protocol_wire::{Direction, RequestTerm, ResultTerm};
use ultragraphdb_store_quads::is_skolem;

use crate::types::names::Names;
use crate::types::row_view::RowView;

/// The system IDs and object a row of a list names, by its keyspace: a forward list's object is its record, a reverse list's its
/// key.
#[derive(Debug)]
pub(crate) struct RowIds {
    pub(crate) graph: Graph,
    pub(crate) subject: Option<EncodedTerm>,
    pub(crate) predicate: Option<SystemId>,
    pub(crate) object: EncodedTerm,
    pub(crate) statement: Option<SystemId>,
}

impl RowIds {
    /// What `row` names, `None` when its key or encoding is not one a row is read from.
    pub(crate) fn of(row: &RowView<'_>) -> Option<Self> {
        let encoded = EncodedTerm::from_bytes(row.encoding).ok()?;
        Some(match row.parsed {
            QuadKey::Forward {
                graph,
                predicate,
                subject,
                ..
            } => Self {
                graph: *graph,
                subject: Some(EncodedTerm::id(*subject)),
                predicate: Some(*predicate),
                object: encoded,
                statement: row.statement,
            },
            QuadKey::Reverse {
                graph,
                predicate,
                object,
                ..
            } => Self {
                graph: *graph,
                subject: Some(encoded),
                predicate: Some(*predicate),
                object: object.clone(),
                statement: row.statement,
            },
            QuadKey::Statement { graph, .. } => Self {
                graph: *graph,
                subject: None,
                predicate: row.property,
                object: encoded,
                statement: None,
            },
            QuadKey::Catalog { .. }
            | QuadKey::SubjectPredicates { .. }
            | QuadKey::ObjectPredicates { .. }
            | QuadKey::ValueIndex { .. }
            | QuadKey::CountIndex { .. }
            | QuadKey::VectorIndex { .. }
            | QuadKey::Statistics { .. }
            | QuadKey::Mark { .. }
            | QuadKey::Referrers { .. }
            | QuadKey::Dictionary { .. } => return None,
        })
    }
}

/// The text the literal's `text` names, inline or from the dictionary.
fn text_of(text: Text<'_>, names: &Names) -> Result<String, TermError> {
    match text {
        Text::Inline(inline) => Ok(inline.to_string()),
        Text::Hashed(hash) => names
            .of_hash(hash)
            .map(str::to_string)
            .ok_or_else(|| TermError::malformed(hash.as_bytes().to_vec(), "no dictionary entry")),
    }
}

/// The term of the IRI or blank node `id` names.
pub(crate) fn id_term(tenant: u64, id: SystemId, names: &Names) -> Option<ResultTerm> {
    let text = names.of_id(id)?;
    let blank = id.type_prefix() == TypePrefix::RESOURCE && is_skolem(tenant, text);
    Some(if blank {
        ResultTerm::blank_node(text)
    } else {
        ResultTerm::iri(text)
    })
}

/// The reifier's term of statement `id`: its skolem IRI, as a blank node.
pub(crate) fn reifier_term(id: SystemId, names: &Names) -> Option<ResultTerm> {
    names.of_id(id).map(ResultTerm::blank_node)
}

/// The literal's encoding as a Result holds it: a datatype by the hash of its IRI, not its system ID (`PROTOCOL.md` §10).
fn hashed_datatype(term: &EncodedTerm, iri: &str) -> Option<EncodedTerm> {
    let (tag, rest) = term.as_bytes().split_first()?;
    let tail = rest.get(SystemId::BYTES..)?;
    let hash = Hash128::of(iri.as_bytes());
    let mut bytes = Vec::with_capacity(term.as_bytes().len());
    bytes.push(*tag);
    bytes.extend_from_slice(hash.as_bytes());
    bytes.extend_from_slice(tail);
    EncodedTerm::from_bytes(&bytes).ok()
}

/// The term of the encoded object `term`, named by `names`, in a Result's encoding (`PROTOCOL.md` §10.3); `None` when a text it
/// names is not held, which is a dangling ID or hash.
pub(crate) fn result_term(tenant: u64, term: &EncodedTerm, names: &Names) -> Option<ResultTerm> {
    let view = term.view().ok()?;
    let hashed = |text: Text<'_>| match text {
        Text::Inline(_) => Some(None),
        Text::Hashed(hash) => names.of_hash(hash).map(|held| Some(held.to_string())),
    };
    Some(match view {
        TermView::Id(id) => return id_term(tenant, id, names),
        TermView::String(text) => with_texts(ResultTerm::new(term.clone()), hashed(text)?, None),
        TermView::LangString {
            lexical, language, ..
        } => with_texts(
            ResultTerm::new(term.clone()),
            hashed(lexical)?,
            hashed(language)?,
        ),
        TermView::Typed { datatype, lexical } => match datatype {
            Datatype::Known(_) => with_texts(ResultTerm::new(term.clone()), hashed(lexical)?, None),
            Datatype::Id(id) => {
                let iri = names.of_id(id)?;
                let encoded = hashed_datatype(term, iri)?;
                with_texts(
                    ResultTerm::new(encoded),
                    hashed(lexical)?,
                    Some(iri.to_string()),
                )
            }
        },
        TermView::Boolean(_)
        | TermView::Integer(_)
        | TermView::Decimal(_)
        | TermView::Double(_)
        | TermView::Float(_)
        | TermView::DateTime(_)
        | TermView::Time(_)
        | TermView::Date(_)
        | TermView::GYearMonth(_)
        | TermView::GYear(_)
        | TermView::GMonthDay(_)
        | TermView::GDay(_)
        | TermView::GMonth(_)
        | TermView::Duration(_)
        | TermView::YearMonthDuration(_)
        | TermView::DayTimeDuration(_)
        | TermView::Vector(_) => ResultTerm::new(term.clone()),
    })
}

fn with_texts(term: ResultTerm, text: Option<String>, annotation: Option<String>) -> ResultTerm {
    let term = match text {
        Some(text) => term.with_text(text),
        None => term,
    };
    match annotation {
        Some(annotation) => term.with_annotation(annotation),
        None => term,
    }
}

/// The term of the encoded object `term` as a resume key's table names it: an IRI or skolem IRI by its text, a literal by its
/// lexical form and datatype; `None` as for [`result_term`].
pub(crate) fn request_term(term: &EncodedTerm, names: &Names) -> Option<RequestTerm> {
    let view = term.view().ok()?;
    if let TermView::Id(id) = view {
        return names
            .of_id(id)
            .map(|text| RequestTerm::Iri(text.to_string()));
    }
    let literal: Literal = view
        .to_literal(
            |id| {
                names
                    .of_id(id)
                    .map(NamedNode::new_unchecked)
                    .ok_or_else(|| {
                        TermError::malformed(id.to_be_bytes().to_vec(), "no dictionary entry")
                    })
            },
            |hash| text_of(Text::Hashed(hash), names),
        )
        .ok()?;
    Some(literal_request_term(&literal))
}

/// `literal` by its parts.
fn literal_request_term(literal: &Literal) -> RequestTerm {
    let lexical = literal.value().to_string();
    if let Some(language) = literal.language() {
        return RequestTerm::LanguageLiteral {
            lexical,
            language: language.to_string(),
            direction: literal.direction().map(|direction| match direction {
                BaseDirection::Ltr => Direction::Ltr,
                BaseDirection::Rtl => Direction::Rtl,
            }),
        };
    }
    let datatype = literal.datatype();
    RequestTerm::Literal {
        lexical,
        datatype: (datatype != xsd::STRING).then(|| datatype.as_str().to_string()),
    }
}
