/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::TenantRun;
use ultragraphdb_core_ids::{Bucket, CanonicalKey, Tenant, TypePrefix};
use ultragraphdb_core_mapping::Names;
use ultragraphdb_core_terms::SYSTEM_VOCABULARY;

/// The path a skolem IRI takes under its tenant's base.
const GENID: &str = ".well-known/genid/";

/// Where an IRI a request names lies, in the request's tenant (`PROTOCOL.md` §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IriPlace {
    /// An ordinary IRI: in the bucket its text derives.
    Ordinary(Bucket),
    /// A skolem IRI of the tenant, in the bucket its text states; whether anyone holds it is that partition's to say.
    Skolem(Bucket),
    /// Under a skolem base but no one's: another tenant's, a tail that is not `{bucket}-{random}`, a bucket outside the run.
    Nobody,
    /// Past the longest IRI a key derives from.
    TooLong,
}

/// Where `iri` lies in `tenant`. The base is compared exactly, so the same path under another base is an ordinary IRI.
#[must_use]
pub fn iri_place(tenant: u64, iri: &str) -> IriPlace {
    if let Some(named) = skolem_base_tenant(iri) {
        if named != tenant {
            return IriPlace::Nobody;
        }
        let base = format!("{SYSTEM_VOCABULARY}tenant/{tenant}/");
        let bucket = Names::new(base)
            .ok()
            .and_then(|names| names.skolem_bucket(iri));
        return match bucket {
            Some(bucket) if TenantRun::of_bucket(bucket) == TenantRun::of(Tenant::new(tenant)) => {
                IriPlace::Skolem(bucket)
            }
            Some(_) | None => IriPlace::Nobody,
        };
    }
    match CanonicalKey::iri(iri) {
        Ok(key) => IriPlace::Ordinary(Bucket::derive(
            Tenant::new(tenant),
            TypePrefix::RESOURCE,
            &key,
        )),
        Err(_) => IriPlace::TooLong,
    }
}

/// The tenant whose skolem base `iri` lies under, when it names one and then the skolem path.
fn skolem_base_tenant(iri: &str) -> Option<u64> {
    let rest = iri
        .strip_prefix(SYSTEM_VOCABULARY)?
        .strip_prefix("tenant/")?;
    let (number, tail) = rest.split_once('/')?;
    if !tail.starts_with(GENID) {
        return None;
    }
    number.parse().ok()
}
