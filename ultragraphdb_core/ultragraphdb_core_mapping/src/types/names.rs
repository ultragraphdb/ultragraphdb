/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_ids::Bucket;

use crate::errors::mapping_error::MappingError;
use crate::types::random_source::RandomSource;

/// The path a skolem IRI takes under its base.
const GENID: &str = ".well-known/genid/";

/// The random bits a skolem IRI holds.
const RANDOM_BITS: u32 = 104;

/// How the property-graph view names IRIs: a label, a property or an edge type `name` is the IRI of the base and the
/// name, and a name holding a colon is an absolute IRI of its own. An IRI under the base is named by what follows the
/// base, any other by itself.
///
/// The base is an absolute IRI (RFC 3987) with no query and no fragment, ending in `/`. Names and skolem IRIs are
/// appended to it as they are, so a name is a path segment beneath it, and a skolem IRI keeps the
/// `/.well-known/genid/` path PROTOCOL.md sections 4.4 and 10.2 recognise it by.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Names {
    base: String,
}

impl Names {
    /// The names under `base`, refused with [`MappingError::invalid_base`] when it is not an absolute IRI, holds a
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
            Some(reason) => Err(MappingError::invalid_base(base, reason)),
            None => Ok(Self { base }),
        }
    }

    /// The base IRI.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The IRI `name` names: `name` itself when it holds a `:`, else `name` under the base.
    pub fn iri(&self, name: &str) -> NamedNode {
        if name.contains(':') {
            NamedNode::new_unchecked(name)
        } else {
            NamedNode::new_unchecked(format!("{}{name}", self.base))
        }
    }

    /// The name of `iri`: the part after the base when `iri` is under it and that part is a name, else the whole IRI.
    pub fn name(&self, iri: &NamedNode) -> String {
        match iri.as_str().strip_prefix(&self.base) {
            Some(name) if !name.is_empty() && !name.contains(':') => name.to_string(),
            _ => iri.as_str().to_string(),
        }
    }

    /// The skolem IRI `{base}.well-known/genid/{bucket}-{random}` of a blank node, a property-graph node created
    /// without an IRI or a statement's reifier, as RDF 1.1 recommends for skolem IRIs: the bucket in 6 lowercase
    /// hexadecimal digits and the low 104 bits of `random` in 26, both zero-padded. It holds no part of a system ID
    /// beyond its bucket.
    pub fn skolem_iri(&self, bucket: Bucket, random: u128) -> NamedNode {
        let random = random & ((1 << RANDOM_BITS) - 1);
        let iri = format!("{}{GENID}{:06x}-{random:026x}", self.base, bucket.get());
        debug_assert_eq!(
            iri.len(),
            self.base.len() + GENID.len() + 33,
            "a fixed-width tail"
        );
        NamedNode::new_unchecked(iri)
    }

    /// A new skolem IRI in `bucket`, its 104 random bits drawn from `random`.
    pub fn mint_skolem(&self, bucket: Bucket, random: &mut RandomSource) -> NamedNode {
        self.skolem_iri(bucket, random.next_bits_104())
    }

    /// The bucket a skolem IRI under this base states, when `iri` is one: the base, `.well-known/genid/`, 6 lowercase
    /// hexadecimal digits, `-` and 26 more, and nothing else.
    pub fn skolem_bucket(&self, iri: &str) -> Option<Bucket> {
        let tail = iri.strip_prefix(self.base.as_str())?.strip_prefix(GENID)?;
        let (bucket, random) = tail.split_once('-')?;
        let lower_hex = |s: &str, len: usize| {
            s.len() == len
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if !(lower_hex(bucket, 6) && lower_hex(random, 26)) {
            return None;
        }
        Bucket::new(u32::from_str_radix(bucket, 16).ok()?).ok()
    }
}
