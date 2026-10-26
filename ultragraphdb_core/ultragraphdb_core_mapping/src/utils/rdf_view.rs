/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use oxrdf::vocab::rdf;
use oxrdf::{NamedNode, Quad, Term, Triple};
use ultragraphdb_core_terms::SYSTEM_VOCABULARY;

use crate::errors::mapping_error::MappingError;
use crate::types::change::Change;
use crate::types::changeset::Changeset;
use crate::types::statement::Statement;
use crate::types::statement_id::StatementId;

/// The reifier a statement ID presents as in the RDF view: `ugdb:statement/<id>`.
pub fn statement_iri(id: StatementId) -> NamedNode {
    NamedNode::new_unchecked(format!("{SYSTEM_VOCABULARY}statement/{}", id.value()))
}

/// The RDF view of `statements`: each triple as one quad, however many statements hold it, and each statement ID as
/// a reifier of its triple, `rdf:reifies` a triple term as RDF 1.2 defines it, annotated with the statement's
/// properties. Quads are ordered by their N-Quads text.
pub fn rdf_view<'a>(statements: impl IntoIterator<Item = &'a Statement>) -> Vec<Quad> {
    let mut quads: BTreeMap<String, Quad> = BTreeMap::new();
    let mut add = |quad: Quad| {
        quads.insert(quad.to_string(), quad);
    };
    for s in statements {
        add(Quad::new(
            s.subject().clone(),
            s.predicate().clone(),
            s.object().clone(),
            s.graph().clone(),
        ));
        if let Some(id) = s.id() {
            let reifier = statement_iri(id);
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
    namespace: u64,
    quads: impl IntoIterator<Item = &'a Quad>,
) -> Result<Changeset, MappingError> {
    let mut changeset = Changeset::new(namespace);
    for quad in quads {
        if let Term::Triple(t) = &quad.object {
            return Err(MappingError::UnsupportedObject(t.to_string()));
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
