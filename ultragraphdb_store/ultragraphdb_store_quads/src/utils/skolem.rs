/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::BlankNode;
use ultragraphdb_core_ids::{Bucket, CanonicalKey, Tenant, TypePrefix};
use ultragraphdb_core_mapping::Names;

use crate::errors::quad_error::QuadError;
use crate::types::write_terms::tenant_base;

/// The path every skolem IRI takes under its tenant's base.
const GENID: &str = "/.well-known/genid/";

/// The names of `tenant`, under its skolem base.
pub(crate) fn names(tenant: u64) -> Result<Names, QuadError> {
    Ok(Names::new(tenant_base(tenant))?)
}

/// Whether `iri` is a skolem IRI under `tenant`'s base, as a blank node or a reifier is named.
pub fn is_skolem(tenant: u64, iri: &str) -> bool {
    // The substring first, so an IRI that names no skolem builds no names.
    iri.contains(GENID) && names(tenant).is_ok_and(|n| n.skolem_bucket(iri).is_some())
}

/// The bucket of `iri` in `tenant`: the bucket a skolem IRI of the tenant states, an existing blank node's or
/// reifier's, or the one the IRI's key derives; `None` for an IRI no key is derived from, which no write resolves.
pub fn iri_bucket(tenant: u64, iri: &str) -> Option<Bucket> {
    // The substring first, so an IRI that names no skolem builds no names.
    if iri.contains(GENID)
        && let Some(bucket) = names(tenant).ok().and_then(|n| n.skolem_bucket(iri))
    {
        return Some(bucket);
    }
    let key = CanonicalKey::iri(iri).ok()?;
    Some(Bucket::derive(
        Tenant::new(tenant),
        TypePrefix::RESOURCE,
        &key,
    ))
}

/// The blank node the skolem IRI `skolem` names, as every read presents it: labelled `b`, then what follows the IRI's
/// last `/`, its bucket, `-` and its random digits, so the label holds no part of the system ID but its bucket.
pub fn skolem_blank_node(skolem: &str) -> BlankNode {
    let tail = skolem.rsplit('/').next().unwrap_or(skolem);
    BlankNode::new_unchecked(format!("b{tail}"))
}
