/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::catalog_error::{CatalogError, CatalogErrorEnum};

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            CatalogErrorEnum::LiteralOnEdge { predicate, object } => {
                write!(
                    f,
                    "Predicate <{predicate}> is declared an edge and cannot hold the literal {object}"
                )
            }
            CatalogErrorEnum::ResourceOnProperty { predicate, object } => {
                write!(
                    f,
                    "Predicate <{predicate}> is declared a property and cannot hold {object}"
                )
            }
            CatalogErrorEnum::DatatypeMismatch {
                predicate,
                expected,
                found,
            } => write!(
                f,
                "Predicate <{predicate}> holds literals of <{expected}>, not of <{found}>"
            ),
            CatalogErrorEnum::UnknownTokenizer {
                predicate,
                tokenizer,
            } => {
                write!(
                    f,
                    "Predicate <{predicate}> declares the unknown tokenizer {tokenizer}"
                )
            }
            CatalogErrorEnum::TokenizerDatatype {
                predicate,
                tokenizer,
                datatype,
            } => write!(
                f,
                "Predicate <{predicate}> declares the tokenizer {tokenizer}, which takes no literal of <{datatype}>"
            ),
            CatalogErrorEnum::UnknownVectorIndex { predicate, factory } => {
                write!(
                    f,
                    "Predicate <{predicate}> declares the unknown vector index {factory}"
                )
            }
            CatalogErrorEnum::SecondParent {
                entity_type,
                parent,
                other,
            } => write!(
                f,
                "Type <{entity_type}> has the parent <{parent}> and cannot take a second, <{other}>"
            ),
            CatalogErrorEnum::ParentDropped {
                entity_type: t,
                parent: p,
            } => parent_dropped(f, t, p),
            CatalogErrorEnum::ParentCycle {
                entity_type,
                parent,
            } => write!(
                f,
                "Type <{entity_type}> cannot take the parent <{parent}>, which descends from it"
            ),
            CatalogErrorEnum::Store(error) => write!(f, "{error}"),
            CatalogErrorEnum::CorruptedVersion { .. } | CatalogErrorEnum::TooLarge { .. } => {
                encoding(f, &self.0)
            }
        }
    }
}

/// The text of a refusal of a declaration without the parent its type has.
fn parent_dropped(f: &mut fmt::Formatter<'_>, entity_type: &str, parent: &str) -> fmt::Result {
    write!(
        f,
        "Type <{entity_type}> has the parent <{parent}> and cannot be declared without it"
    )
}

/// The text of a refusal of the catalog's encoding: a version that does not decode, a declaration too long to encode.
fn encoding(f: &mut fmt::Formatter<'_>, error: &CatalogErrorEnum) -> fmt::Result {
    if let CatalogErrorEnum::CorruptedVersion {
        namespace,
        version,
        reason,
    } = error
    {
        write!(
            f,
            "Catalog version {version} of namespace {namespace} does not decode: {reason}"
        )
    } else if let CatalogErrorEnum::TooLarge {
        iri,
        what,
        len,
        max,
    } = error
    {
        write!(
            f,
            "The declaration of <{iri}> holds {len} {what}, at most {max}"
        )
    } else {
        write!(f, "{error:?}")
    }
}
