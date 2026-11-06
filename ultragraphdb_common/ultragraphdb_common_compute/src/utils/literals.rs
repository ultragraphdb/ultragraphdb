/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use oxrdf::{BaseDirection, Literal, NamedNode};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR, KnownDatatype, TermError};
use ultragraphdb_protocol_wire::{Direction, RequestTerm};

/// The literal `term` names, `None` for a term that is no literal.
pub(crate) fn literal_of(term: &RequestTerm) -> Option<Literal> {
    match term {
        RequestTerm::Literal { lexical, datatype } => Some(match datatype {
            None => Literal::new_simple_literal(lexical),
            Some(iri) => {
                Literal::new_typed_literal(lexical, NamedNode::new_unchecked(iri.as_str()))
            }
        }),
        RequestTerm::LanguageLiteral {
            lexical,
            language,
            direction,
        } => Some(match direction {
            None => Literal::new_language_tagged_literal_unchecked(lexical, language.as_str()),
            Some(Direction::Ltr) => Literal::new_directional_language_tagged_literal_unchecked(
                lexical,
                language.as_str(),
                BaseDirection::Ltr,
            ),
            Some(Direction::Rtl) => Literal::new_directional_language_tagged_literal_unchecked(
                lexical,
                language.as_str(),
                BaseDirection::Rtl,
            ),
        }),
        RequestTerm::Iri(_) | RequestTerm::BlankNode(_) | RequestTerm::Triple { .. } => None,
    }
}

/// The IRI of a datatype the store names `term` by its system ID, which the dictionary gives: a literal of a datatype neither
/// `xsd:string` nor known nor the vector's.
pub(crate) fn datatype_named(term: &RequestTerm) -> Option<&str> {
    let RequestTerm::Literal {
        datatype: Some(iri),
        ..
    } = term
    else {
        return None;
    };
    let by_id = iri.as_str() != xsd::STRING.as_str()
        && iri.as_str() != FLOAT32_VECTOR
        && KnownDatatype::from_iri(iri).is_none();
    by_id.then_some(iri.as_str())
}

/// Whether `term` is a vector, which no reverse key names.
pub(crate) fn is_vector(term: &RequestTerm) -> bool {
    matches!(
        term,
        RequestTerm::Literal { datatype: Some(iri), .. } if iri.as_str() == FLOAT32_VECTOR
    )
}

/// The store's encoding of the literal `term`, `datatype` the ID [`datatype_named`]'s IRI has; `None` for a term that is no
/// literal, and for a literal naming a datatype by an ID not given.
pub(crate) fn encoding_of(
    term: &RequestTerm,
    datatype: Option<SystemId>,
) -> Result<Option<EncodedTerm>, TermError> {
    let Some(literal) = literal_of(term) else {
        return Ok(None);
    };
    if datatype_named(term).is_some() && datatype.is_none() {
        return Ok(None);
    }
    let encoded = EncodedTerm::literal(literal.as_ref(), |iri| {
        datatype.ok_or_else(|| TermError::malformed(iri.as_str().as_bytes().to_vec(), "unnamed"))
    })?;
    Ok(Some(encoded.into_term()))
}
