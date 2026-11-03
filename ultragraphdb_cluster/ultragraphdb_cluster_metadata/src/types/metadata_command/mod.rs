/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

pub(crate) mod metadata_command_codec;

use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId};
use ultragraphdb_core_ids::{Epoch, Namespace, TypePrefix};
use ultragraphdb_protocol_scram::ScramVerifier;

/// One command of the metadata group's log, the body of one round. Each records its decision whole, so replaying it
/// needs nothing but the state before it: a lease names its block, a principal its identity, a registration the
/// sessions it evicts. Its encoding is [`MetadataCommand::encode`]'s.
///
/// A command whose encoding passes [`MetadataCommand::CHUNK_BYTES`] is written as a run of
/// [`MetadataCommand::Chunk`]s, each one round holding the next piece of its encoding, and a
/// [`MetadataCommand::Commit`] naming the whole by its length and checksum: the command takes effect at the commit,
/// and a run with no commit after it is no command.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MetadataCommand {
    /// A new version of `namespace`'s catalog, numbered `predecessor + 1`, built on version `predecessor`, holding
    /// `bytes`; it is pending until a close announces it.
    OfferCatalog {
        /// The namespace whose catalog it is.
        namespace: Namespace,
        /// The version it is built on, 0 for the first.
        predecessor: u64,
        /// The version's bytes, in `ultragraphdb_core_catalog`'s encoding.
        bytes: Vec<u8>,
    },
    /// The default entry `entry` a first write of the undeclared `predicate` in `namespace` asks for; no catalog
    /// version.
    RecordDefault {
        /// The namespace written.
        namespace: Namespace,
        /// The predicate's name.
        predicate: String,
        /// The default entry, in `ultragraphdb_core_catalog`'s encoding.
        entry: Vec<u8>,
    },
    /// The block `start..end` of `prefix`'s sequences leased to `partition`, starting at the type's high-water mark.
    LeaseBlock {
        /// The type whose sequences are leased.
        prefix: TypePrefix,
        /// The partition leasing them.
        partition: PartitionId,
        /// The block's first sequence.
        start: u64,
        /// The sequence after its last.
        end: u64,
    },
    /// The placement map's first layout: every namespace run split evenly over the partitions 1 to `partitions`, each
    /// led by `leader`.
    InitPlacement {
        /// The partitions the buckets are split over.
        partitions: u32,
        /// The preferred leader of every partition.
        leader: NodeId,
    },
    /// A layout of the placement map applying from `from`: every bucket range assigned to its partition.
    ChangePlacement {
        /// The epoch the layout applies from.
        from: Epoch,
        /// The ranges and their partitions, in any order.
        assignments: Vec<(BucketRange, PartitionId)>,
    },
    /// `node` made the preferred leader of `partition`.
    SetLeader {
        /// The partition.
        partition: PartitionId,
        /// Its preferred leader.
        node: NodeId,
    },
    /// The principal `id` named `name`, authenticating with `verifier`, holding the administrator role when
    /// `administrator`.
    CreatePrincipal {
        /// The principal's identity, one past the last created.
        id: u64,
        /// Its name.
        name: String,
        /// Its SCRAM-SHA-256 verifier.
        verifier: ScramVerifier,
        /// Whether it holds the administrator role.
        administrator: bool,
    },
    /// The principal named `name` removed.
    RemovePrincipal {
        /// The principal's name.
        name: String,
    },
    /// The session `session` registered for the principal `principal`, evicting `evicts`, the least recently
    /// registered sessions, when the registry holds `sessions_max`.
    RegisterSession {
        /// The session's 128-bit identity.
        session: u128,
        /// The principal it acts for.
        principal: u64,
        /// The sessions it evicts, oldest first.
        evicts: Vec<u128>,
    },
    /// The sequencer's durable close of `epoch`, announcing `versions`, which take effect at the epoch after it, and
    /// carrying `evictions`.
    LearnClose {
        /// The epoch closed.
        epoch: Epoch,
        /// The catalog versions the close announces.
        versions: Vec<(Namespace, u64)>,
        /// The session evictions the close carries.
        evictions: Vec<u128>,
    },
    /// `partition`'s untrimmed log begins at `epoch`: no catalog version superseded at or before it is named there.
    ReportUntrimmed {
        /// The partition.
        partition: PartitionId,
        /// The oldest epoch its untrimmed log may name.
        epoch: Epoch,
    },
    /// The piece of a large command's encoding starting at byte `offset`; offset 0 begins a new run.
    Chunk {
        /// Where the piece lies in the command's encoding.
        offset: u64,
        /// The piece, at most [`MetadataCommand::CHUNK_BYTES`].
        bytes: Vec<u8>,
    },
    /// The end of a run of chunks: the command they hold is `length` bytes long with the XXH3-64 checksum `checksum`.
    Commit {
        /// The command's encoded length.
        length: u64,
        /// The XXH3-64 of the command's encoding.
        checksum: u64,
    },
}

impl MetadataCommand {
    /// The most bytes of a catalog version, 16 MiB, written in chunks.
    pub const CATALOG_BYTES_MAX: usize = 1 << 24;

    /// The most bytes of a command's encoding, 32 MiB.
    pub const COMMAND_BYTES_MAX: usize = 1 << 25;

    /// The most bytes of one chunk, and of a command written as one round, 128 KiB: a round's frame holds 1 MiB, and a
    /// test log's segment 256 KiB.
    pub const CHUNK_BYTES: usize = 1 << 17;

    /// The most bytes of a predicate's name, 4,096.
    pub const PREDICATE_BYTES_MAX: usize = 4_096;

    /// The most bytes of a default entry, 65,536.
    pub const ENTRY_BYTES_MAX: usize = 1 << 16;

    /// The most ranges one placement change assigns, the placement map's most ranges, 2^18.
    pub const ASSIGNMENTS_MAX: usize = 1 << 18;

    /// The most bytes of a principal's name, 255.
    pub const NAME_BYTES_MAX: usize = 255;

    /// The most bytes of a verifier's salt, 255.
    pub const SALT_BYTES_MAX: usize = 255;

    /// The most catalog versions one close announces, the sequencer's limit, 256.
    pub const VERSIONS_MAX: usize = 256;

    /// The most session evictions one close carries, the sequencer's limit, 4,096.
    pub const EVICTIONS_MAX: usize = 4_096;
}
