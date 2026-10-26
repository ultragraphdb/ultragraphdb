/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::TermRef;
use ultragraphdb_core_mapping::{PredicateDeclaration, PredicateKind};

use crate::errors::catalog_error::CatalogError;
use crate::types::catalog::Catalog;

impl Catalog {
    /// Checks `object` as an object of predicate `declaration` declares: a literal on an edge, an IRI or blank node on
    /// a property, and a literal of another datatype than the constraint are refused, naming the predicate and the
    /// object.
    pub fn check_object(
        declaration: &PredicateDeclaration,
        object: TermRef<'_>,
    ) -> Result<(), CatalogError> {
        let predicate = declaration.predicate().as_str();
        match (object, declaration.kind()) {
            (TermRef::Literal(literal), PredicateKind::Edge) => {
                Err(CatalogError::LiteralOnEdge(predicate, literal.to_string()))
            }
            (TermRef::Literal(literal), _) => match declaration.datatype() {
                Some(expected) if literal.datatype() != expected.as_ref() => {
                    Err(CatalogError::DatatypeMismatch(
                        predicate,
                        expected.as_str(),
                        literal.datatype().as_str(),
                    ))
                }
                _ => Ok(()),
            },
            (other, PredicateKind::Property) => Err(CatalogError::ResourceOnProperty(
                predicate,
                other.to_string(),
            )),
            _ => Ok(()),
        }
    }
}

/// The datatype a literal `object` has, `xsd:string` for a simple literal; `None` for an IRI or a blank node.
pub fn datatype_of(object: TermRef<'_>) -> Option<&str> {
    match object {
        TermRef::Literal(l) => Some(l.datatype().as_str()),
        _ => None,
    }
}
