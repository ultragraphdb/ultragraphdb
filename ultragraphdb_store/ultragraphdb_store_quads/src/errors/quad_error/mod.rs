/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod quad_error_display;
mod quad_error_error;
mod quad_error_from;

use ultragraphdb_core_catalog::CatalogError;
use ultragraphdb_core_ids::{IdError, TypePrefix};
use ultragraphdb_core_keys::QuadKeyError;
use ultragraphdb_core_mapping::MappingError;
use ultragraphdb_core_terms::TermError;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_posting::PostingError;

/// Why an operation of the quad store failed. The classification is the public field `.0`; branch on it.
///
/// - **Carried:** [`Store`](QuadErrorEnum::Store), [`Posting`](QuadErrorEnum::Posting), [`Term`](QuadErrorEnum::Term),
///   [`Key`](QuadErrorEnum::Key), [`Mapping`](QuadErrorEnum::Mapping), [`Catalog`](QuadErrorEnum::Catalog),
///   [`Id`](QuadErrorEnum::Id): a layer below refused.
/// - **Damage:** [`Corrupted`](QuadErrorEnum::Corrupted): a stored value or dictionary entry is missing or unreadable.
/// - **Allocation:** [`NoSequence`](QuadErrorEnum::NoSequence), [`PendingFull`](QuadErrorEnum::PendingFull),
///   [`NoOwnedBucket`](QuadErrorEnum::NoOwnedBucket), [`SkolemDraws`](QuadErrorEnum::SkolemDraws): no ID or name can be
///   given now; nothing is written, and the write may be retried.
/// - **Refused name:** [`UnknownSkolem`](QuadErrorEnum::UnknownSkolem): an IRI under the skolem base the dictionary does
///   not hold.
/// - **Another partition's:** [`NotOwned`](QuadErrorEnum::NotOwned): an IRI of a bucket the partition does not own and
///   holds no ID for; its home registers it first.
#[derive(Debug, Clone, PartialEq)]
pub enum QuadErrorEnum {
    /// The store failed.
    Store(StoreError),
    /// A posting list could not be read or written.
    Posting(PostingError),
    /// A term could not be encoded or decoded.
    Term(TermError),
    /// A quad key could not be parsed.
    Key(QuadKeyError),
    /// A statement maps to nothing the store holds.
    Mapping(MappingError),
    /// The catalog could not be read.
    Catalog(CatalogError),
    /// A system ID or an IRI's canonical key was refused.
    Id(IdError),
    /// A stored ID or literal hash names no dictionary entry, or an entry cannot be read: `key` holds it.
    Corrupted {
        /// The key whose entry is missing or unreadable.
        key: Vec<u8>,
        /// What is wrong with it.
        reason: String,
    },
    /// The partition holds no sequence of the type `prefix` and the block source leased none.
    NoSequence {
        /// The type's prefix.
        prefix: u16,
    },
    /// The IDs created and not yet committed number `capacity`, all the store holds, so no other is created now.
    PendingFull {
        /// The most uncommitted IDs the store holds.
        capacity: usize,
    },
    /// The partition owns no bucket of tenant `tenant`'s run, so it mints no blank node of it.
    NoOwnedBucket {
        /// The tenant.
        tenant: u64,
    },
    /// Each of `draws` skolem IRIs drawn in turn named an IRI the dictionary holds.
    SkolemDraws {
        /// The draws made.
        draws: usize,
    },
    /// `iri` lies under the tenant's skolem base and the dictionary holds no blank node or reifier by it.
    UnknownSkolem {
        /// The IRI.
        iri: String,
    },
    /// `iri`'s bucket `bucket` is another partition's and the partition holds no ID for it: its home gives the ID
    /// (term-dictionary, "A reference to another partition's entity registers at its home").
    NotOwned {
        /// The IRI.
        iri: String,
        /// Its bucket.
        bucket: u32,
    },
}

/// See [`QuadErrorEnum`].
#[derive(Debug, Clone, PartialEq)]
pub struct QuadError(pub QuadErrorEnum);

impl QuadError {
    /// [`QuadErrorEnum::Corrupted`] for `key`, with `reason`.
    pub fn corrupted(key: &[u8], reason: impl Into<String>) -> Self {
        Self(QuadErrorEnum::Corrupted {
            key: key.to_vec(),
            reason: reason.into(),
        })
    }

    /// [`QuadErrorEnum::NoSequence`] for the type `prefix`.
    #[must_use]
    pub fn no_sequence(prefix: TypePrefix) -> Self {
        Self(QuadErrorEnum::NoSequence {
            prefix: prefix.get(),
        })
    }

    /// [`QuadErrorEnum::PendingFull`] at `capacity`.
    #[must_use]
    pub fn pending_full(capacity: usize) -> Self {
        Self(QuadErrorEnum::PendingFull { capacity })
    }

    /// [`QuadErrorEnum::NoOwnedBucket`] for `tenant`.
    pub fn no_owned_bucket(tenant: u64) -> Self {
        Self(QuadErrorEnum::NoOwnedBucket { tenant })
    }

    /// [`QuadErrorEnum::NotOwned`] for `iri`, of bucket `bucket`.
    pub fn not_owned(iri: impl Into<String>, bucket: u32) -> Self {
        Self(QuadErrorEnum::NotOwned {
            iri: iri.into(),
            bucket,
        })
    }

    /// [`QuadErrorEnum::SkolemDraws`] after `draws` draws.
    #[must_use]
    pub fn skolem_draws(draws: usize) -> Self {
        Self(QuadErrorEnum::SkolemDraws { draws })
    }

    /// [`QuadErrorEnum::UnknownSkolem`] for `iri`.
    pub fn unknown_skolem(iri: impl Into<String>) -> Self {
        Self(QuadErrorEnum::UnknownSkolem { iri: iri.into() })
    }
}
