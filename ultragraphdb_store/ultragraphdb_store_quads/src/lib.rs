/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Quads on posting lists: the term dictionary over the keyspace without versions, UID and statement-ID allocation,
//! quad reads and writes, statistics, and the data dictionary's read API.

mod errors;
mod types;
mod utils;

pub use crate::errors::quad_error::{QuadError, QuadErrorEnum};
pub use crate::types::data_dictionary::DataDictionary;
pub use crate::types::description::Description;
pub use crate::types::label_shape::LabelShape;
pub use crate::types::leases::{LEASE_BLOCK, Leases, MAX_UID};
pub use crate::types::property_shape::PropertyShape;
pub use crate::types::quad_store::QuadStore;
pub use crate::types::quad_write::QuadWrite;
pub use crate::types::shapes::Shapes;
pub use crate::types::statement_record::StatementRecord;
pub use crate::types::statistics::{SKETCH_REGISTERS, Statistics};
pub use crate::types::write_terms::{WriteTerms, namespace_base};
pub use crate::utils::dictionary_keys::{
    CONTRACT_KEY, HASH_TO_STRING, IRI_TO_UID, STATEMENT_LEASE, UID_LEASE, UID_TO_IRI, iri_key,
    lease_key, string_key, uid_key, uid_value,
};
pub use crate::utils::postings::{object_posting, posting_object};
