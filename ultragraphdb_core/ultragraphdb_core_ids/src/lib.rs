/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The system ID and the versions every partition stamps, with no I/O.
//!
//! A [`SystemId`] names every entity and statement inside the database in 128 bits, from the most significant: a
//! 16-bit [`TypePrefix`], a 64-bit [`Sequence`], a 24-bit [`Bucket`] and 24 reserved bits, which are zero. It is
//! stored and compared as an unsigned big-endian value, so IDs of one type sort together and, within a type, by
//! sequence then bucket. An entity's bucket is computed, never chosen ([`Bucket::derive`]): 8 bits hashed from its
//! [`Tenant`] and 16 from its declaring type's prefix and its [`CanonicalKey`]. A statement takes its subject's
//! bucket. Until UGS keys exist, an IRI is the key of the built-in `Resource` type ([`TypePrefix::RESOURCE`]) and
//! every statement is of the built-in `Statement` type ([`TypePrefix::STATEMENT`]).
//!
//! A [`Version`] is 64 bits: a 42-bit [`Epoch`], which the sequencer closes, and a 22-bit sequence within it, even
//! for a commit. A [`SessionToken`] names a commit by its partition, its log position and its epoch.

// Arithmetic on decoded wire/store input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

mod errors;
mod types;

pub use crate::errors::id_error::{IdError, IdErrorEnum};

pub use crate::types::bucket::Bucket;
pub use crate::types::bucket_group::BucketGroup;
pub use crate::types::canonical_key::CanonicalKey;
pub use crate::types::epoch::Epoch;
pub use crate::types::sequence::Sequence;
pub use crate::types::session_token::SessionToken;
pub use crate::types::system_id::SystemId;
pub use crate::types::tenant::Tenant;
pub use crate::types::type_prefix::TypePrefix;
pub use crate::types::version::Version;
