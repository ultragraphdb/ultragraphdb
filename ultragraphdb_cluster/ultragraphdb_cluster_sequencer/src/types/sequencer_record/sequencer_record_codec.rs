/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_error_cluster::ClusterError;

use crate::types::catalog_version::CatalogVersion;
use crate::types::close::Close;
use crate::types::reader::Reader;
use crate::types::sequenced_transaction::SequencedTransaction;
use crate::types::sequencer_record::SequencerRecord;

/// The kind byte of a close.
const CLOSE: u8 = 1;

/// The kind byte of a transaction.
const TRANSACTION: u8 = 2;

impl SequencerRecord {
    /// Appends the record's encoding to `out`, big-endian throughout. A close: the kind byte 1, the epoch and the time
    /// as 8 bytes each, the catalog versions' count as 2 and each version's tenant and number as 8 each, the
    /// evictions' count as 2 and each session as 16, the placement version as 8, 0 for none. A transaction: the kind byte 2, the epoch and the reply epoch as 8
    /// bytes each, the partitions' count as 2 and each partition as 4, the body's length as 4 and its bytes. A record
    /// has one encoding.
    pub fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Close(close) => {
                out.push(CLOSE);
                out.extend_from_slice(&close.epoch().get().to_be_bytes());
                out.extend_from_slice(&close.time_us().to_be_bytes());
                out.extend_from_slice(&count_u16(close.catalog().len()).to_be_bytes());
                for version in close.catalog() {
                    out.extend_from_slice(&version.tenant().get().to_be_bytes());
                    out.extend_from_slice(&version.version().to_be_bytes());
                }
                out.extend_from_slice(&count_u16(close.evictions().len()).to_be_bytes());
                for session in close.evictions() {
                    out.extend_from_slice(&session.to_be_bytes());
                }
                out.extend_from_slice(&close.placement().to_be_bytes());
            }
            Self::Transaction(transaction) => {
                out.push(TRANSACTION);
                out.extend_from_slice(&transaction.epoch().get().to_be_bytes());
                out.extend_from_slice(&transaction.reply_epoch().get().to_be_bytes());
                out.extend_from_slice(&count_u16(transaction.partitions().len()).to_be_bytes());
                for partition in transaction.partitions() {
                    out.extend_from_slice(&partition.get().to_be_bytes());
                }
                // At most 65,536 bytes, so the length fits its 4 bytes.
                let body = transaction.body();
                out.extend_from_slice(&u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes());
                out.extend_from_slice(body);
            }
        }
    }

    /// The record `bytes` encode, exactly as [`SequencerRecord::encode`] writes it.
    ///
    /// # Errors
    ///
    /// [`ClusterError::malformed`] naming the first value that is not as the encoding writes it, and its offset: an
    /// unknown kind byte, an epoch past 42 bits, partition 0, a truncated value, bytes after the end; and the refusals
    /// of [`Close::new`](crate::Close::new) and [`SequencedTransaction::new`](crate::SequencedTransaction::new) for
    /// the values decoded, a count past its limit refused before the values it counts are read.
    pub fn decode(bytes: &[u8]) -> Result<Self, ClusterError> {
        let mut reader = Reader::new(bytes);
        let record = match reader.u8("a kind byte, 1 or 2")? {
            CLOSE => Self::Close(read_close(&mut reader)?),
            TRANSACTION => Self::Transaction(read_transaction(&mut reader)?),
            _unknown => return Err(ClusterError::malformed("a kind byte, 1 or 2", 0)),
        };
        reader.finish()?;
        Ok(record)
    }
}

/// `count`, at most a close's or a transaction's limit, as its 2-byte field.
fn count_u16(count: usize) -> u16 {
    debug_assert!(count <= Close::EVICTIONS_MAX);
    u16::try_from(count).unwrap_or(u16::MAX)
}

/// The epoch at the reader, refused past 42 bits.
fn read_epoch(reader: &mut Reader<'_>, what: &'static str) -> Result<Epoch, ClusterError> {
    let at = reader.offset();
    let epoch = reader.u64(what)?;
    Epoch::new(epoch).map_err(|_| ClusterError::malformed("an epoch of 42 bits", at))
}

/// A count at the reader, refused past `max` as `what` too many.
fn read_count(
    reader: &mut Reader<'_>,
    what: &'static str,
    max: usize,
) -> Result<usize, ClusterError> {
    let count = usize::from(reader.u16(what)?);
    if count > max {
        return Err(ClusterError::too_many(what, count, max));
    }
    Ok(count)
}

/// A close's fields after its kind byte.
fn read_close(reader: &mut Reader<'_>) -> Result<Close, ClusterError> {
    let epoch = read_epoch(reader, "a close's epoch")?;
    let time_us = reader.u64("a close's time")?;
    let versions = read_count(reader, "catalog versions in a close", Close::CATALOG_MAX)?;
    let mut catalog = Vec::with_capacity(versions);
    for _ in 0..versions {
        let tenant = Tenant::new(reader.u64("a catalog version's tenant")?);
        catalog.push(CatalogVersion::new(
            tenant,
            reader.u64("a catalog version's number")?,
        ));
    }
    let sessions = read_count(reader, "session evictions in a close", Close::EVICTIONS_MAX)?;
    let mut evictions = Vec::with_capacity(sessions);
    for _ in 0..sessions {
        evictions.push(reader.u128("an evicted session")?);
    }
    let placement = reader.u64("a close's placement version")?;
    Ok(Close::new(epoch, time_us, catalog, evictions)?.with_placement(placement))
}

/// A transaction's fields after its kind byte.
fn read_transaction(reader: &mut Reader<'_>) -> Result<SequencedTransaction, ClusterError> {
    let epoch = read_epoch(reader, "a transaction's epoch")?;
    let reply_epoch = read_epoch(reader, "a transaction's reply epoch")?;
    let count = read_count(
        reader,
        "partitions of a transaction",
        SequencedTransaction::PARTITIONS_MAX,
    )?;
    let mut partitions = Vec::with_capacity(count);
    for _ in 0..count {
        let at = reader.offset();
        let partition = reader.u32("a partition from 1")?;
        partitions.push(
            PartitionId::new(partition)
                .map_err(|_| ClusterError::malformed("a partition from 1", at))?,
        );
    }
    // A `u32` length, held whole by a `usize` of 32 bits or more, as on every target the node builds for.
    let len = reader.u32("a transaction's body length")? as usize;
    if len > SequencedTransaction::BODY_BYTES_MAX {
        return Err(ClusterError::too_many(
            "bytes of a transaction's body",
            len,
            SequencedTransaction::BODY_BYTES_MAX,
        ));
    }
    let body = reader.bytes(len, "a transaction's body")?.to_vec();
    SequencedTransaction::new(epoch, reply_epoch, partitions, body)
}
