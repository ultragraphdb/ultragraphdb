/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use oxrdf::vocab::rdf;
use oxrdf::{Quad, Term, Triple};

use crate::errors::mapping_error::MappingError;
use crate::types::change::Change;
use crate::types::changeset::Changeset;
use crate::types::statement::Statement;

/// The RDF view of `statements`: each triple as one quad, however many statements hold it, and each statement's
/// reifier as a reifier of its triple, `rdf:reifies` a triple term as RDF 1.2 defines it, annotated with the statement's
/// properties. Quads are ordered by their N-Quads text.
pub fn rdf_view<'a>(statements: impl IntoIterator<Item = &'a Statement>) -> Vec<Quad> {
    let mut quads: BTreeMap<String, Quad> = BTreeMap::new();
    let mut add = |quad: Quad| {
        let _held: Option<Quad> = quads.insert(quad.to_string(), quad);
    };
    for s in statements {
        add(Quad::new(
            s.subject().clone(),
            s.predicate().clone(),
            s.object().clone(),
            s.graph().clone(),
        ));
        if let Some(reifier) = s.reifier() {
            let triple = Triple::new(
                s.subject().clone(),
                s.predicate().clone(),
                s.object().clone(),
            );
            add(Quad::new(
                reifier.clone(),
                rdf::REIFIES,
                Term::Triple(Box::new(triple)),
                s.graph().clone(),
            ));
            for (predicate, value) in s.properties() {
                add(Quad::new(
                    reifier.clone(),
                    predicate.clone(),
                    value.clone(),
                    s.graph().clone(),
                ));
            }
        }
    }
    quads.into_values().collect()
}

/// The changeset that writes `quads` through the RDF view: each quad inserted, held once however often it is named.
/// A quad whose object is a triple term is refused; a statement's properties are written as statement properties.
pub fn quads_to_changeset<'a>(
    tenant: u64,
    quads: impl IntoIterator<Item = &'a Quad>,
) -> Result<Changeset, MappingError> {
    let mut changeset = Changeset::new(tenant);
    for quad in quads {
        if let Term::Triple(t) = &quad.object {
            return Err(MappingError::unsupported_object(t.to_string()));
        }
        changeset.push(Change::Insert {
            graph: quad.graph_name.clone(),
            subject: quad.subject.clone(),
            predicate: quad.predicate.clone(),
            object: quad.object.clone(),
        });
    }
    Ok(changeset)
}
