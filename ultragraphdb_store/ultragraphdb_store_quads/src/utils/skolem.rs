/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::BlankNode;
use ultragraphdb_core_mapping::Names;

use crate::errors::quad_error::QuadError;
use crate::types::write_terms::namespace_base;

/// The path every skolem IRI takes under its namespace's base.
const GENID: &str = "/.well-known/genid/";

/// The names of `namespace`, under its skolem base.
pub(crate) fn names(namespace: u64) -> Result<Names, QuadError> {
    Ok(Names::new(namespace_base(namespace))?)
}

/// Whether `iri` is a skolem IRI under `namespace`'s base, as a blank node or a reifier is named.
pub(crate) fn is_skolem(namespace: u64, iri: &str) -> bool {
    // The substring first, so an IRI that names no skolem builds no names.
    iri.contains(GENID) && names(namespace).is_ok_and(|n| n.skolem_bucket(iri).is_some())
}

/// The blank node the skolem IRI `skolem` names, as every read presents it: labelled `b`, then what follows the IRI's
/// last `/`, its bucket, `-` and its random digits, so the label holds no part of the system ID but its bucket.
pub fn skolem_blank_node(skolem: &str) -> BlankNode {
    let tail = skolem.rsplit('/').next().unwrap_or(skolem);
    BlankNode::new_unchecked(format!("b{tail}"))
}
