/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId};
use ultragraphdb_core_ids::{Epoch, Namespace, TypePrefix};
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_protocol_scram::ScramVerifier;

use crate::types::metadata_command::MetadataCommand;
use crate::types::metadata_state::MetadataState;
use crate::types::reader::Reader;

/// What a command's kind byte must be.
const KIND: &str = "a kind byte, 1 to 13";

impl MetadataCommand {
    /// Appends the command's encoding to `out`, big-endian throughout: its kind byte, 1 to 13 in declaration order, then
    /// its fields in order. A namespace, an epoch, a sequence and a version number take 8 bytes, a type prefix 2, a
    /// partition, a node, a bucket and a partition count 4, a session 16, a principal's identity 8, a flag 1 (0 or 1);
    /// bytes and text take their length (4 bytes for a catalog version and an entry, 2 for a predicate, 1 for a name and
    /// a salt) and then themselves; a verifier is its salt, its iteration count as 4 bytes and its two keys of 32; a
    /// list takes its count as 4 bytes and then its items. A command has one encoding.
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.kind());
        match self {
            Self::OfferCatalog {
                namespace,
                predecessor,
                bytes,
            } => {
                put_u64(out, namespace.get());
                put_u64(out, *predecessor);
                put_len_u32(out, bytes);
            }
            Self::RecordDefault {
                namespace,
                predicate,
                entry,
            } => {
                put_u64(out, namespace.get());
                // At most PREDICATE_BYTES_MAX, which fits 2 bytes, once the state took it.
                let len = u16::try_from(predicate.len()).unwrap_or(u16::MAX);
                out.extend_from_slice(&len.to_be_bytes());
                out.extend_from_slice(predicate.as_bytes());
                put_len_u32(out, entry);
            }
            Self::LeaseBlock {
                prefix,
                partition,
                start,
                end,
            } => {
                out.extend_from_slice(&prefix.get().to_be_bytes());
                put_u32(out, partition.get());
                put_u64(out, *start);
                put_u64(out, *end);
            }
            Self::InitPlacement { partitions, leader } => {
                put_u32(out, *partitions);
                put_u32(out, leader.get());
            }
            Self::ChangePlacement { assignments } => {
                put_count(out, assignments.len());
                for (range, partition) in assignments {
                    put_u32(out, range.start());
                    put_u32(out, range.end());
                    put_u32(out, partition.get());
                }
            }
            Self::SetLeader { partition, node } => {
                put_u32(out, partition.get());
                put_u32(out, node.get());
            }
            Self::CreatePrincipal { .. }
            | Self::RemovePrincipal { .. }
            | Self::RegisterSession { .. }
            | Self::LearnClose { .. }
            | Self::ReportUntrimmed { .. }
            | Self::Chunk { .. }
            | Self::Commit { .. } => self.encode_rest(out),
        }
    }

    /// The encodings of the access and epoch commands, after their kind byte.
    fn encode_rest(&self, out: &mut Vec<u8>) {
        match self {
            Self::CreatePrincipal {
                id,
                name,
                verifier,
                administrator,
            } => {
                put_u64(out, *id);
                put_len_u8(out, name.as_bytes());
                put_len_u8(out, verifier.salt());
                put_u32(out, verifier.iterations());
                out.extend_from_slice(verifier.stored_key());
                out.extend_from_slice(verifier.server_key());
                out.push(u8::from(*administrator));
            }
            Self::RemovePrincipal { name, evicts } => {
                put_len_u8(out, name.as_bytes());
                put_sessions(out, evicts);
            }
            Self::RegisterSession {
                session,
                principal,
                evicts,
            } => {
                out.extend_from_slice(&session.to_be_bytes());
                put_u64(out, *principal);
                put_sessions(out, evicts);
            }
            Self::LearnClose {
                epoch,
                versions,
                placement,
                evictions,
            } => {
                put_u64(out, epoch.get());
                put_count(out, versions.len());
                for (namespace, version) in versions {
                    put_u64(out, namespace.get());
                    put_u64(out, *version);
                }
                put_sessions(out, evictions);
                put_u64(out, *placement);
            }
            Self::ReportUntrimmed { partition, epoch } => {
                put_u32(out, partition.get());
                put_u64(out, epoch.get());
            }
            Self::Chunk { offset, bytes } => {
                put_u64(out, *offset);
                put_len_u32(out, bytes);
            }
            Self::Commit { length, checksum } => {
                put_u64(out, *length);
                put_u64(out, *checksum);
            }
            Self::OfferCatalog { .. }
            | Self::RecordDefault { .. }
            | Self::LeaseBlock { .. }
            | Self::InitPlacement { .. }
            | Self::ChangePlacement { .. }
            // encode writes these itself and never passes them here.
            | Self::SetLeader { .. } => {}
        }
    }

    /// The command's kind byte, 1 to 13 in declaration order.
    fn kind(&self) -> u8 {
        match self {
            Self::OfferCatalog { .. } => 1,
            Self::RecordDefault { .. } => 2,
            Self::LeaseBlock { .. } => 3,
            Self::InitPlacement { .. } => 4,
            Self::ChangePlacement { .. } => 5,
            Self::SetLeader { .. } => 6,
            Self::CreatePrincipal { .. } => 7,
            Self::RemovePrincipal { .. } => 8,
            Self::RegisterSession { .. } => 9,
            Self::LearnClose { .. } => 10,
            Self::ReportUntrimmed { .. } => 11,
            Self::Chunk { .. } => 12,
            Self::Commit { .. } => 13,
        }
    }

    /// The command `bytes` encode, exactly as [`MetadataCommand::encode`] writes it.
    ///
    /// # Errors
    ///
    /// [`ClusterError::malformed`] naming the first value that is not as the encoding writes it, and its offset: an
    /// unknown kind byte, an epoch past 42 bits, a type prefix or a partition of 0, a bucket range that is empty or
    /// past the last bucket, a flag other than 0 or 1, text that is not UTF-8, a truncated value, bytes after the end;
    /// [`ClusterError::too_many`] for a length or a count past its limit, refused before what it counts is read.
    pub fn decode(bytes: &[u8]) -> Result<Self, ClusterError> {
        let mut reader = Reader::new(bytes);
        let command = match reader.u8(KIND)? {
            1 => Self::OfferCatalog {
                namespace: Namespace::new(reader.u64("a namespace")?),
                predecessor: reader.u64("a predecessor")?,
                bytes: read_len_u32(
                    &mut reader,
                    "bytes of a catalog version",
                    Self::CATALOG_BYTES_MAX,
                )?,
            },
            2 => read_default(&mut reader)?,
            3 => Self::LeaseBlock {
                prefix: read_prefix(&mut reader)?,
                partition: read_partition(&mut reader)?,
                start: reader.u64("a block's start")?,
                end: reader.u64("a block's end")?,
            },
            4 => Self::InitPlacement {
                partitions: reader.u32("a partition count")?,
                leader: NodeId::new(reader.u32("a node")?),
            },
            5 => read_change(&mut reader)?,
            6 => Self::SetLeader {
                partition: read_partition(&mut reader)?,
                node: NodeId::new(reader.u32("a node")?),
            },
            7 => read_create(&mut reader)?,
            8 => Self::RemovePrincipal {
                name: read_name(&mut reader)?,
                evicts: read_sessions(
                    &mut reader,
                    "session evictions of a removal",
                    MetadataState::EVICTIONS_PENDING_MAX,
                )?,
            },
            9 => Self::RegisterSession {
                session: reader.u128("a session")?,
                principal: reader.u64("a principal")?,
                evicts: read_sessions(&mut reader, "session evictions", Self::EVICTIONS_MAX)?,
            },
            10 => read_close(&mut reader)?,
            11 => Self::ReportUntrimmed {
                partition: read_partition(&mut reader)?,
                epoch: read_epoch(&mut reader)?,
            },
            12 => Self::Chunk {
                offset: reader.u64("a chunk's offset")?,
                bytes: read_len_u32(&mut reader, "bytes of a chunk", Self::CHUNK_BYTES)?,
            },
            13 => Self::Commit {
                length: reader.u64("a command's length")?,
                checksum: reader.u64("a command's checksum")?,
            },
            _unknown => return Err(ClusterError::malformed(KIND, 0)),
        };
        reader.finish("the end of the command")?;
        Ok(command)
    }
}

/// Appends `value` as 8 bytes.
pub(crate) fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Appends `value` as 4 bytes.
pub(crate) fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Appends `count`, at most a table's limit, as 4 bytes.
pub(crate) fn put_count(out: &mut Vec<u8>, count: usize) {
    put_u32(out, u32::try_from(count).unwrap_or(u32::MAX));
}

/// Appends `bytes` after their length as 4 bytes.
pub(crate) fn put_len_u32(out: &mut Vec<u8>, bytes: &[u8]) {
    put_count(out, bytes.len());
    out.extend_from_slice(bytes);
}

/// Appends `bytes`, at most 255, after their length as 1 byte.
pub(crate) fn put_len_u8(out: &mut Vec<u8>, bytes: &[u8]) {
    out.push(u8::try_from(bytes.len()).unwrap_or(u8::MAX));
    out.extend_from_slice(bytes);
}

/// Appends `sessions` after their count as 4 bytes, 16 bytes each.
fn put_sessions(out: &mut Vec<u8>, sessions: &[u128]) {
    put_count(out, sessions.len());
    for session in sessions {
        out.extend_from_slice(&session.to_be_bytes());
    }
}

/// Bytes after their length as 4 bytes, refused past `max` as `what`.
pub(crate) fn read_len_u32(
    reader: &mut Reader<'_>,
    what: &'static str,
    max: usize,
) -> Result<Vec<u8>, ClusterError> {
    let len = Reader::bounded(reader.u32(what)? as usize, what, max)?;
    Ok(reader.bytes(len, what)?.to_vec())
}

/// An epoch, refused past 42 bits.
pub(crate) fn read_epoch(reader: &mut Reader<'_>) -> Result<Epoch, ClusterError> {
    let at = reader.offset();
    Epoch::new(reader.u64("an epoch")?)
        .map_err(|_| ClusterError::malformed("an epoch of 42 bits", at))
}

/// A type prefix, refused at 0.
pub(crate) fn read_prefix(reader: &mut Reader<'_>) -> Result<TypePrefix, ClusterError> {
    let at = reader.offset();
    TypePrefix::new(reader.u16("a type prefix from 1")?)
        .map_err(|_| ClusterError::malformed("a type prefix from 1", at))
}

/// A partition, refused at 0.
pub(crate) fn read_partition(reader: &mut Reader<'_>) -> Result<PartitionId, ClusterError> {
    let at = reader.offset();
    PartitionId::new(reader.u32("a partition from 1")?)
        .map_err(|_| ClusterError::malformed("a partition from 1", at))
}

/// A principal's name after its length as 1 byte.
pub(crate) fn read_name(reader: &mut Reader<'_>) -> Result<String, ClusterError> {
    let len = usize::from(reader.u8("a principal's name")?);
    reader.text(len, "a principal's name")
}

/// A verifier: its salt after its length as 1 byte, its iteration count and its two keys.
pub(crate) fn read_verifier(reader: &mut Reader<'_>) -> Result<ScramVerifier, ClusterError> {
    let len = usize::from(reader.u8("a verifier's salt")?);
    let salt = reader.bytes(len, "a verifier's salt")?.to_vec();
    let iterations = reader.u32("a verifier's iteration count")?;
    let stored = reader.bytes(32, "a verifier's stored key")?;
    let server = reader.bytes(32, "a verifier's server key")?;
    let mut stored_key = [0; 32];
    stored_key.copy_from_slice(stored);
    let mut server_key = [0; 32];
    server_key.copy_from_slice(server);
    Ok(ScramVerifier::new(salt, iterations, stored_key, server_key))
}

/// Sessions after their count as 4 bytes, refused past `max` as `what`.
fn read_sessions(
    reader: &mut Reader<'_>,
    what: &'static str,
    max: usize,
) -> Result<Vec<u128>, ClusterError> {
    let count = Reader::bounded(reader.u32(what)? as usize, what, max)?;
    (0..count).map(|_| reader.u128("a session")).collect()
}

/// A RecordDefault's fields.
fn read_default(reader: &mut Reader<'_>) -> Result<MetadataCommand, ClusterError> {
    let namespace = Namespace::new(reader.u64("a namespace")?);
    let what = "bytes of a predicate's name";
    let len = Reader::bounded(
        usize::from(reader.u16(what)?),
        what,
        MetadataCommand::PREDICATE_BYTES_MAX,
    )?;
    let predicate = reader.text(len, what)?;
    let entry = read_len_u32(
        reader,
        "bytes of a default entry",
        MetadataCommand::ENTRY_BYTES_MAX,
    )?;
    Ok(MetadataCommand::RecordDefault {
        namespace,
        predicate,
        entry,
    })
}

/// A ChangePlacement's fields.
fn read_change(reader: &mut Reader<'_>) -> Result<MetadataCommand, ClusterError> {
    let what = "ranges of a placement change";
    let count = Reader::bounded(
        reader.u32(what)? as usize,
        what,
        MetadataCommand::ASSIGNMENTS_MAX,
    )?;
    let mut assignments = Vec::with_capacity(count);
    for _ in 0..count {
        let at = reader.offset();
        let start = reader.u32("a range's first bucket")?;
        let end = reader.u32("a range's end")?;
        let range = BucketRange::new(start, end)
            .map_err(|_| ClusterError::malformed("a bucket range within 2^24", at))?;
        assignments.push((range, read_partition(reader)?));
    }
    Ok(MetadataCommand::ChangePlacement { assignments })
}

/// A CreatePrincipal's fields.
fn read_create(reader: &mut Reader<'_>) -> Result<MetadataCommand, ClusterError> {
    Ok(MetadataCommand::CreatePrincipal {
        id: reader.u64("a principal")?,
        name: read_name(reader)?,
        verifier: read_verifier(reader)?,
        administrator: reader.flag()?,
    })
}

/// A LearnClose's fields.
fn read_close(reader: &mut Reader<'_>) -> Result<MetadataCommand, ClusterError> {
    let epoch = read_epoch(reader)?;
    let what = "catalog versions in a close";
    let count = Reader::bounded(
        reader.u32(what)? as usize,
        what,
        MetadataCommand::VERSIONS_MAX,
    )?;
    let mut versions = Vec::with_capacity(count);
    for _ in 0..count {
        let namespace = Namespace::new(reader.u64("a namespace")?);
        versions.push((namespace, reader.u64("a version")?));
    }
    let evictions = read_sessions(
        reader,
        "session evictions in a close",
        MetadataCommand::EVICTIONS_MAX,
    )?;
    let placement = reader.u64("a placement version")?;
    Ok(MetadataCommand::LearnClose {
        epoch,
        versions,
        placement,
        evictions,
    })
}
