/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_cluster_placement::{BucketRange, PlacementMap};
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_traits_log::Position;

use crate::types::catalog_record::CatalogRecord;
use crate::types::metadata_command::MetadataCommand;
use crate::types::metadata_command::metadata_command_codec::{
    put_count, put_len_u8, put_len_u32, put_u32, put_u64, read_epoch, read_len_u32, read_name,
    read_partition, read_prefix, read_verifier,
};
use crate::types::metadata_config::MetadataConfig;
use crate::types::metadata_snapshot::MetadataSnapshot;
use crate::types::metadata_state::{MetadataState, TenantCatalog};
use crate::types::principal::Principal;
use crate::types::reader::Reader;

/// The format byte of the encoding.
const FORMAT: u8 = 1;

/// The most catalogs a snapshot holds: one per tenant, 2^20, as many as default-entry records.
const CATALOGS_MAX: usize = MetadataState::DEFAULTS_MAX;

/// The most high-water marks: one per type prefix.
const PREFIXES_MAX: usize = 1 << 16;

/// The most bytes of an encoded placement map, 256 layouts of 2^18 ranges.
const PLACEMENT_BYTES_MAX: usize = 1 << 31;

/// The most partitions reporting, the placement map's.
const REPORTS_MAX: usize = PlacementMap::PARTITIONS_MAX as usize;

impl MetadataSnapshot {
    /// Appends the snapshot's encoding to `out`, big-endian throughout, fields as [`crate::MetadataCommand::encode`]
    /// writes them: the format byte 1 and the position as 8 bytes; the last learned close as 8; the catalogs' count and
    /// per tenant its number, its latest and highest released versions and its retained versions' count, then per
    /// version its number, a flag and, when set, its effective epoch, and its bytes; the default-entry records' count
    /// and each tenant, predicate and entry; the high-water marks' count and each prefix and next sequence; a flag
    /// and, when set, the placement map's length as 4 bytes and its own encoding; the reports' count and each partition
    /// and epoch; the next principal's identity, the principals' count and each identity, name, verifier and
    /// administrator flag; the sessions' count and each session and principal in registration order; the pending
    /// evictions' count and each session; the last placement version's number as 8 bytes, the pending placement
    /// versions' count and each version's number, its ranges' count and each range's first bucket, end and partition.
    /// Every table is written in its order, so a state has one encoding.
    pub fn encode(&self, out: &mut Vec<u8>) {
        let state = self.state();
        out.push(FORMAT);
        put_u64(out, self.position().get());
        put_u64(out, state.learned.get());
        encode_catalogs(state, out);
        encode_tables(state, out);
        encode_access(state, out);
        encode_placement_pending(state, out);
    }

    /// The snapshot `bytes` encode, exactly as [`MetadataSnapshot::encode`] writes it.
    ///
    /// # Errors
    ///
    /// [`ClusterError::malformed`] naming the first value that is not as the encoding writes it, and its offset: a
    /// format byte other than 1, a value out of its order or range, a placement map its own decoder refuses, a
    /// truncated value, bytes after the end; [`ClusterError::too_many`] for a count past its table's limit, refused
    /// before what it counts is read.
    pub fn decode(bytes: &[u8]) -> Result<Self, ClusterError> {
        let mut reader = Reader::new(bytes);
        if reader.u8("the format byte 1")? != FORMAT {
            return Err(ClusterError::malformed("the format byte 1", 0));
        }
        let position = Position::new(reader.u64("a position")?);
        let mut state = MetadataState::new();
        state.learned = read_epoch(&mut reader)?;
        decode_catalogs(&mut reader, &mut state)?;
        decode_tables(&mut reader, &mut state)?;
        decode_access(&mut reader, &mut state)?;
        decode_placement_pending(&mut reader, &mut state)?;
        reader.finish("the end of the snapshot")?;
        Ok(Self::new(position, state))
    }
}

/// The catalogs: per tenant its versions.
fn encode_catalogs(state: &MetadataState, out: &mut Vec<u8>) {
    put_count(out, state.catalogs.len());
    for (tenant, catalog) in &state.catalogs {
        put_u64(out, tenant.get());
        put_u64(out, catalog.current);
        put_u64(out, catalog.released);
        put_count(out, catalog.versions.len());
        for record in &catalog.versions {
            put_u64(out, record.version());
            match record.effective() {
                Some(epoch) => {
                    out.push(1);
                    put_u64(out, epoch.get());
                }
                None => out.push(0),
            }
            put_len_u32(out, record.bytes());
        }
    }
}

/// The default entries, the high-water marks, the placement map and the reports.
fn encode_tables(state: &MetadataState, out: &mut Vec<u8>) {
    put_count(out, state.defaults.len());
    for ((tenant, predicate), entry) in &state.defaults {
        put_u64(out, tenant.get());
        // At most PREDICATE_BYTES_MAX, which fits 2 bytes.
        let len = u16::try_from(predicate.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(predicate.as_bytes());
        put_len_u32(out, entry);
    }
    put_count(out, state.high_water.len());
    for (prefix, next) in &state.high_water {
        out.extend_from_slice(&prefix.to_be_bytes());
        put_u64(out, *next);
    }
    match &state.placement {
        Some(map) => {
            out.push(1);
            let mut encoded = Vec::new();
            map.encode(&mut encoded);
            put_len_u32(out, &encoded);
        }
        None => out.push(0),
    }
    put_count(out, state.untrimmed.len());
    for (partition, epoch) in &state.untrimmed {
        put_u32(out, partition.get());
        put_u64(out, epoch.get());
    }
}

/// The principals, the sessions and the pending evictions.
fn encode_access(state: &MetadataState, out: &mut Vec<u8>) {
    put_u64(out, state.next_principal);
    put_count(out, state.principals.len());
    for principal in state.principals.values() {
        put_u64(out, principal.id());
        put_len_u8(out, principal.name().as_bytes());
        let verifier = principal.verifier();
        put_len_u8(out, verifier.salt());
        put_u32(out, verifier.iterations());
        out.extend_from_slice(verifier.stored_key());
        out.extend_from_slice(verifier.server_key());
        out.push(u8::from(principal.administrator()));
    }
    put_count(out, state.sessions.len());
    for (session, principal) in &state.sessions {
        out.extend_from_slice(&session.to_be_bytes());
        put_u64(out, *principal);
    }
    put_count(out, state.evictions.len());
    for session in &state.evictions {
        out.extend_from_slice(&session.to_be_bytes());
    }
}

/// The last placement version's number and the pending placement versions, oldest first.
fn encode_placement_pending(state: &MetadataState, out: &mut Vec<u8>) {
    put_u64(out, state.placement_version);
    put_count(out, state.placement_pending.len());
    for (version, layout) in &state.placement_pending {
        put_u64(out, *version);
        put_count(out, layout.len());
        for (range, partition) in layout {
            put_u32(out, range.start());
            put_u32(out, range.end());
            put_u32(out, partition.get());
        }
    }
}

/// The pending placement versions, held only beside a map, each numbered one above the one before, the last numbered
/// as the last placement version.
fn decode_placement_pending(
    reader: &mut Reader<'_>,
    state: &mut MetadataState,
) -> Result<(), ClusterError> {
    state.placement_version = reader.u64("the last placement version")?;
    let count_at = reader.offset();
    let pending = count(
        reader,
        "pending placement versions",
        MetadataState::PLACEMENT_PENDING_MAX,
    )?;
    check(
        pending == 0 || state.placement.is_some(),
        "no pending placement version without a placement map",
        count_at,
    )?;
    let first = state
        .placement_version
        .checked_sub(pending as u64)
        .map(|before| before + 1);
    for index in 0..pending {
        let version_at = reader.offset();
        let version = reader.u64("a placement version")?;
        check(
            first.map(|first| first + index as u64) == Some(version),
            "a placement version one above the one before, the last the last committed",
            version_at,
        )?;
        let ranges = count(
            reader,
            "ranges of a placement change",
            MetadataCommand::ASSIGNMENTS_MAX,
        )?;
        let mut layout = Vec::with_capacity(ranges);
        for _ in 0..ranges {
            let range_at = reader.offset();
            let start = reader.u32("a range's first bucket")?;
            let end = reader.u32("a range's end")?;
            let range = BucketRange::new(start, end)
                .map_err(|_| ClusterError::malformed("a bucket range within 2^24", range_at))?;
            layout.push((range, read_partition(reader)?));
        }
        state.placement_pending.push_back((version, layout));
    }
    Ok(())
}

/// A count at the reader, refused past `max` as `what`.
fn count(reader: &mut Reader<'_>, what: &'static str, max: usize) -> Result<usize, ClusterError> {
    Reader::bounded(reader.u32(what)? as usize, what, max)
}

/// Refuses as malformed `what` at `at` unless `holds`.
fn check(holds: bool, what: &'static str, at: usize) -> Result<(), ClusterError> {
    if holds {
        Ok(())
    } else {
        Err(ClusterError::malformed(what, at))
    }
}

/// The catalogs, each tenant after the one before, its versions rising, effective ones first and rising, the last
/// its latest.
fn decode_catalogs(reader: &mut Reader<'_>, state: &mut MetadataState) -> Result<(), ClusterError> {
    let catalogs = count(reader, "catalogs", CATALOGS_MAX)?;
    for _ in 0..catalogs {
        let at = reader.offset();
        let tenant = Tenant::new(reader.u64("a tenant")?);
        let after = state
            .catalogs
            .keys()
            .next_back()
            .is_none_or(|last| *last < tenant);
        check(after, "a tenant after the one before it", at)?;
        let current_at = reader.offset();
        let current = reader.u64("a tenant's latest version")?;
        let released = reader.u64("a tenant's highest released version")?;
        let what = "retained catalog versions of a tenant";
        let versions = count(reader, what, MetadataState::RETAINED_MAX)?;
        let mut catalog = TenantCatalog {
            current,
            released,
            versions: std::collections::VecDeque::with_capacity(versions),
        };
        for _ in 0..versions {
            let record = read_record(reader, tenant, &catalog)?;
            catalog.versions.push_back(record);
        }
        let last = catalog.versions.back().map(CatalogRecord::version);
        check(
            last == Some(current),
            "a tenant's latest version, its last retained",
            current_at,
        )?;
        let _absent: Option<TenantCatalog> = state.catalogs.insert(tenant, catalog);
    }
    Ok(())
}

/// One retained version of `tenant`, after the versions of `catalog` read before it.
fn read_record(
    reader: &mut Reader<'_>,
    tenant: Tenant,
    catalog: &TenantCatalog,
) -> Result<CatalogRecord, ClusterError> {
    let at = reader.offset();
    let version = reader.u64("a version")?;
    let before = catalog.versions.back();
    let rises = before.map_or(version > catalog.released, |b| version > b.version());
    check(rises, "a version after the one before it", at)?;
    let effective = if reader.flag()? {
        let epoch_at = reader.offset();
        let epoch = read_epoch(reader)?;
        let ordered = before.is_none_or(|b| b.effective().is_some_and(|e| e <= epoch));
        check(
            ordered,
            "an effective epoch at or after the one before it",
            epoch_at,
        )?;
        Some(epoch)
    } else {
        None
    };
    let bytes = read_len_u32(
        reader,
        "bytes of a catalog version",
        MetadataCommand::CATALOG_BYTES_MAX,
    )?;
    Ok(CatalogRecord::new(tenant, version, effective, bytes))
}

/// The default entries, the high-water marks, the placement map and the reports, each table in its order.
fn decode_tables(reader: &mut Reader<'_>, state: &mut MetadataState) -> Result<(), ClusterError> {
    let defaults = count(reader, "default-entry records", MetadataState::DEFAULTS_MAX)?;
    for _ in 0..defaults {
        let at = reader.offset();
        let tenant = Tenant::new(reader.u64("a tenant")?);
        let what = "bytes of a predicate's name";
        let len = Reader::bounded(
            usize::from(reader.u16(what)?),
            what,
            MetadataCommand::PREDICATE_BYTES_MAX,
        )?;
        let key = (tenant, reader.text(len, what)?);
        let after = state
            .defaults
            .keys()
            .next_back()
            .is_none_or(|last| *last < key);
        check(after, "a default entry after the one before it", at)?;
        let entry = read_len_u32(
            reader,
            "bytes of a default entry",
            MetadataCommand::ENTRY_BYTES_MAX,
        )?;
        let _absent: Option<Vec<u8>> = state.defaults.insert(key, entry);
    }
    let marks = count(reader, "high-water marks", PREFIXES_MAX)?;
    for _ in 0..marks {
        let at = reader.offset();
        let prefix = read_prefix(reader)?.get();
        let after = state
            .high_water
            .keys()
            .next_back()
            .is_none_or(|last| *last < prefix);
        check(after, "a type prefix after the one before it", at)?;
        let _absent: Option<u64> = state
            .high_water
            .insert(prefix, reader.u64("a next sequence")?);
    }
    if reader.flag()? {
        let at = reader.offset() + 4;
        let encoded = read_len_u32(reader, "bytes of a placement map", PLACEMENT_BYTES_MAX)?;
        let map = PlacementMap::decode(&encoded).map_err(|error| shifted(&error, at))?;
        state.placement = Some(map);
    }
    let reports = count(reader, "partitions reporting", REPORTS_MAX)?;
    for _ in 0..reports {
        let at = reader.offset();
        let partition = read_partition(reader)?;
        let after = state
            .untrimmed
            .keys()
            .next_back()
            .is_none_or(|last| *last < partition);
        check(after, "a partition after the one before it", at)?;
        let _absent = state.untrimmed.insert(partition, read_epoch(reader)?);
    }
    Ok(())
}

/// `error`, the placement map's refusal of its bytes, its offset moved by `at`, where they begin in the snapshot.
fn shifted(error: &ClusterError, at: usize) -> ClusterError {
    if let ClusterErrorEnum::Malformed { what, offset } = error.0 {
        ClusterError::malformed(what, at + offset)
    } else {
        ClusterError::malformed("a placement map", at)
    }
}

/// The principals by name, below the next identity and each once; the sessions, each once; the pending evictions.
fn decode_access(reader: &mut Reader<'_>, state: &mut MetadataState) -> Result<(), ClusterError> {
    state.next_principal = reader.u64("the next principal")?;
    let principals = count(reader, "principals", MetadataState::PRINCIPALS_MAX)?;
    let mut ids = BTreeSet::new();
    for _ in 0..principals {
        let at = reader.offset();
        let id = reader.u64("a principal")?;
        let name = read_name(reader)?;
        let verifier = read_verifier(reader)?;
        let administrator = reader.flag()?;
        let after = state
            .principals
            .keys()
            .next_back()
            .is_none_or(|last| *last < name);
        let fresh = id < state.next_principal && ids.insert(id) && !name.is_empty();
        check(
            after && fresh,
            "a principal after the one before it, below the next",
            at,
        )?;
        let principal = Principal::new(id, name.clone(), verifier, administrator);
        let _absent: Option<Principal> = state.principals.insert(name, principal);
    }
    let what = "sessions";
    let sessions = count(reader, what, MetadataConfig::SESSIONS_MAX_MAX as usize)?;
    for _ in 0..sessions {
        let at = reader.offset();
        let session = reader.u128("a session")?;
        let principal = reader.u64("a principal")?;
        let fresh = state.owners.insert(session, principal).is_none();
        check(fresh, "a session registered once", at)?;
        state.sessions.push_back((session, principal));
    }
    let evictions = count(
        reader,
        "session evictions awaiting a close",
        MetadataState::EVICTIONS_PENDING_MAX,
    )?;
    for _ in 0..evictions {
        state
            .evictions
            .push_back(reader.u128("an evicted session")?);
    }
    Ok(())
}
