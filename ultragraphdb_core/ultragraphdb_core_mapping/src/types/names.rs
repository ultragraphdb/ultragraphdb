/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;

use crate::errors::mapping_error::MappingError;

/// How the property-graph view names IRIs: a label, a property or an edge type `name` is the IRI of the base and the
/// name, and a name holding a colon is an absolute IRI of its own. An IRI under the base is named by what follows the
/// base, any other by itself.
///
/// The base is an absolute IRI (RFC 3987) with no query and no fragment, ending in `/`. Names, node IRIs and skolem
/// IRIs are appended to it as they are, so a name is a path segment beneath it, and a skolem IRI keeps the
/// `/.well-known/genid/` path PROTOCOL.md sections 4.4 and 10.2 recognise it by.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Names {
    base: String,
}

impl Names {
    /// The names under `base`, refused with [`MappingError::InvalidBase`] when it is not an absolute IRI, holds a
    /// query or a fragment, or does not end in `/`.
    pub fn new(base: impl Into<String>) -> Result<Self, MappingError> {
        let base = base.into();
        let reason = if NamedNode::new(base.as_str()).is_err() {
            Some("not an absolute IRI")
        } else if base.contains(['?', '#']) {
            // In a valid IRI, `?` and `#` appear only in or as the delimiter of a query or a fragment.
            Some("has a query or a fragment")
        } else if !base.ends_with('/') {
            Some("does not end in '/'")
        } else {
            None
        };
        match reason {
            Some(reason) => Err(MappingError::InvalidBase(base, reason)),
            None => Ok(Self { base }),
        }
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
