/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use oxrdf::{Literal, NamedNode, NamedNodeRef};

use crate::errors::term_error::TermError;
use crate::types::datatype::Datatype;
use crate::types::hash128::Hash128;
use crate::types::known_datatype::{FLOAT32_VECTOR, KnownDatatype};
use crate::types::term_view::TermView;
use crate::types::text::Text;
use crate::utils::vector::format_vector;

impl TermView<'_> {
    /// The literal this view holds, exactly as it was encoded: `datatype_iri` gives the IRI of a datatype named by
    /// its UID, and `string` the string a hash names. Refused for a UID, which names an IRI or a blank node.
    pub fn to_literal<E: From<TermError>>(
        &self,
        datatype_iri: impl FnOnce(u64) -> Result<NamedNode, E>,
        mut string: impl FnMut(Hash128) -> Result<String, E>,
    ) -> Result<Literal, E> {
        let mut text = |t: &Text<'_>| match t {
            Text::Inline(s) => Ok(s.to_string()),
            Text::Hashed(h) => string(*h),
        };
        let typed =
            |value: String, datatype: NamedNodeRef<'_>| Literal::new_typed_literal(value, datatype);
        Ok(match self {
            TermView::Uid(uid) => return Err(TermError::NotALiteral(*uid).into()),
            TermView::Boolean(v) => typed(v.to_string(), xsd::BOOLEAN),
            TermView::Integer(v) => typed(v.to_string(), xsd::INTEGER),
            TermView::Decimal(v) => typed(v.to_string(), xsd::DECIMAL),
            TermView::Double(v) => typed(v.to_string(), xsd::DOUBLE),
            TermView::Float(v) => typed(v.to_string(), xsd::FLOAT),
            TermView::DateTime(v) => typed(v.to_string(), xsd::DATE_TIME),
            TermView::Time(v) => typed(v.to_string(), xsd::TIME),
            TermView::Date(v) => typed(v.to_string(), xsd::DATE),
            TermView::GYearMonth(v) => typed(v.to_string(), xsd::G_YEAR_MONTH),
            TermView::GYear(v) => typed(v.to_string(), xsd::G_YEAR),
            TermView::GMonthDay(v) => typed(v.to_string(), xsd::G_MONTH_DAY),
            TermView::GDay(v) => typed(v.to_string(), xsd::G_DAY),
            TermView::GMonth(v) => typed(v.to_string(), xsd::G_MONTH),
            TermView::Duration(v) => typed(v.to_string(), xsd::DURATION),
            TermView::YearMonthDuration(v) => typed(v.to_string(), xsd::YEAR_MONTH_DURATION),
            TermView::DayTimeDuration(v) => typed(v.to_string(), xsd::DAY_TIME_DURATION),
            TermView::String(lexical) => Literal::new_simple_literal(text(lexical)?),
            TermView::LangString {
                lexical,
                language,
                direction,
            } => {
                let (lexical, language) = (text(lexical)?, text(language)?);
                match direction {
                    None => Literal::new_language_tagged_literal_unchecked(lexical, language),
                    Some(d) => Literal::new_directional_language_tagged_literal_unchecked(
                        lexical, language, *d,
                    ),
                }
            }
            TermView::Typed { datatype, lexical } => {
                let lexical = text(lexical)?;
                match datatype {
                    Datatype::Known(k) => typed(lexical, known_iri(*k)),
                    Datatype::Uid(uid) => Literal::new_typed_literal(lexical, datatype_iri(*uid)?),
                }
            }
            TermView::Vector(elements) => typed(
                format_vector(elements.iter()),
                NamedNodeRef::new_unchecked(FLOAT32_VECTOR),
            ),
        })
    }
}

fn known_iri(datatype: KnownDatatype) -> NamedNodeRef<'static> {
    NamedNodeRef::new_unchecked(datatype.iri())
}
