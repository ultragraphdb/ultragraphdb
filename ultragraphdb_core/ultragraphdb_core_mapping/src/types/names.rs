/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;

/// How the property-graph view names IRIs: a label, a property or an edge type `name` is the IRI of the base and the
/// name, and a name holding a colon is an absolute IRI of its own. An IRI under the base is named by what follows the
/// base, any other by itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Names {
    base: String,
}

impl Names {
    pub fn new(base: impl Into<String>) -> Self {
        Self { base: base.into() }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn iri(&self, name: &str) -> NamedNode {
        if name.contains(':') {
            NamedNode::new_unchecked(name)
        } else {
            NamedNode::new_unchecked(format!("{}{name}", self.base))
        }
    }

    pub fn name(&self, iri: &NamedNode) -> String {
        match iri.as_str().strip_prefix(&self.base) {
            Some(name) if !name.is_empty() && !name.contains(':') => name.to_string(),
            _ => iri.as_str().to_string(),
        }
    }

    /// The skolem IRI a blank node is held as, named by its UID under the base's `/.well-known/genid/` path, as
    /// RDF 1.1 recommends for skolem IRIs. The dictionary marks it skolemized, so the RDF view presents it as a blank
    /// node again.
    pub fn skolem_iri(&self, uid: u64) -> NamedNode {
        NamedNode::new_unchecked(format!("{}.well-known/genid/{uid}", self.base))
    }

    /// The IRI of a node the property-graph view creates, named by its UID under the base.
    pub fn node_iri(&self, uid: u64) -> NamedNode {
        NamedNode::new_unchecked(format!("{}node/{uid}", self.base))
    }
}
