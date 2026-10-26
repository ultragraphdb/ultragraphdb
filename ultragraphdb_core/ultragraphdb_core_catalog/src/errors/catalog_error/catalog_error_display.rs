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
            CatalogErrorEnum::ParentCycle {
                entity_type,
                parent,
            } => write!(
                f,
                "Type <{entity_type}> cannot take the parent <{parent}>, which descends from it"
            ),
            CatalogErrorEnum::Decode { key, reason } => {
                let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
                write!(f, "Catalog entry {hex} cannot be read: {reason}")
            }
            CatalogErrorEnum::Store(error) => write!(f, "{error}"),
        }
    }
}
