/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Quads on posting lists: the term dictionary in its keyspace without versions, system-ID allocation from computed
//! buckets and leased sequence blocks, quad reads and writes, statistics, and the data dictionary's read API.
//!
//! An IRI's ID is got or created while the writing execution runs ([`WriteTerms::iri`]): the dictionary's ID, the ID
//! a concurrent execution created and has not yet committed, or a new one, its bucket computed from the tenant and
//! the IRI ([`ultragraphdb_core_ids::Bucket::derive`]) and its sequence drawn from a block the partition leased through
//! [`ultragraphdb_traits_blocks::BlockSource`]. Every writer of one IRI receives one ID and none is refused for it; the
//! dictionary entries are written by every commit naming a term not yet held, so they are durable no later than the
//! first such commit. A blank node, and a statement's reifier, is named by a skolem IRI drawn from the store's random
//! source, drawn again when the dictionary already holds it. The dictionary is naming state: what exists is read from
//! the lists, never from an entry.

mod errors;
mod types;
mod utils;

pub use crate::errors::quad_error::{QuadError, QuadErrorEnum};
pub use crate::types::data_dictionary::DataDictionary;
pub use crate::types::description::Description;
pub use crate::types::kernel_row::KernelRow;
pub use crate::types::kernel_scan::KernelScan;
pub use crate::types::label_shape::LabelShape;
pub use crate::types::owned_buckets::OwnedBuckets;
pub use crate::types::page::Page;
pub use crate::types::page_end::PageEnd;
pub use crate::types::page_walk::PageWalk;
pub use crate::types::property_shape::PropertyShape;
pub use crate::types::quad_store::{PENDING_MAX, QuadStore, SKOLEM_DRAWS_MAX};
pub use crate::types::quad_write::QuadWrite;
pub use crate::types::referrer_member::ReferrerMember;
pub use crate::types::referrer_set::{ReferrerSet, Registered};
pub use crate::types::sequences::Sequences;
pub use crate::types::shapes::Shapes;
pub use crate::types::statement_record::StatementRecord;
pub use crate::types::statistics::{SKETCH_REGISTERS, Statistics};
pub use crate::types::write_terms::{WriteTerms, tenant_base};
pub use crate::utils::dictionary_keys::{CONTRACT_KEY, id_key, literal_key, term_key};
pub use crate::utils::list_objects::list_objects;
pub use crate::utils::postings::{object_posting, posting_object};
pub use crate::utils::skolem::{iri_bucket, is_skolem, skolem_blank_node};
