/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, worker/mutation.go (the entries `updateSchema` and `updateType` write). Changed for
 * ultragraphdb: translated to Rust; the entry is returned for the caller's batch rather than committed alone.
 * Copyright (c) 2026. Marvin Hansen.
 */

use prost::Message;
use ultragraphdb_core_keys::{schema_key, type_key};
use ultragraphdb_proto_pb::pb;
use ultragraphdb_traits_store::Entry;

use crate::errors::schema_error::SchemaError;

/// The meta byte of a stored schema or type, Dgraph's `posting.BitSchemaPosting`.
const BIT_SCHEMA_POSTING: u8 = 0x01;

/// The entry storing `update` at version `ts`, as Dgraph's `updateSchema` writes it: under the predicate's schema
/// key, the update's protobuf encoding, the meta byte `BitSchemaPosting` and the discard flag, so compaction may
/// collect the schema's older versions.
///
/// # Errors
///
/// The [`KeyError`](ultragraphdb_core_keys::KeyError) of a predicate whose namespace does not parse; Dgraph's
/// `SchemaKey` panics on it.
pub fn schema_entry(update: &pb::SchemaUpdate, ts: u64) -> Result<Entry, SchemaError> {
    let key = schema_key(&update.predicate)?;
    Ok(Entry::put(key, ts, BIT_SCHEMA_POSTING, update.encode_to_vec()).with_discard())
}

/// The entry storing `update` at version `ts`, as Dgraph's `updateType` writes it: under the type key, with the
/// meta byte and the discard flag of [`schema_entry`].
///
/// # Errors
///
/// The [`KeyError`](ultragraphdb_core_keys::KeyError) of a type name whose namespace does not parse; Dgraph's
/// `TypeKey` panics on it.
pub fn type_entry(update: &pb::TypeUpdate, ts: u64) -> Result<Entry, SchemaError> {
    let key = type_key(&update.type_name)?;
    Ok(Entry::put(key, ts, BIT_SCHEMA_POSTING, update.encode_to_vec()).with_discard())
}
