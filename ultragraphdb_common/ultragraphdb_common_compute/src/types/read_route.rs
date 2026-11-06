/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PlacementMap;
use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply, ReadLevel, RequestTerm};
use ultragraphdb_store_quads::iri_bucket;

use crate::types::read_request::ReadRequest;
use crate::utils::refusals::{partitions_detail, term_refused};

/// Which partitions a read asks, by its bound positions (`PROTOCOL.md` §11.12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReadRoute {
    /// These partitions, ascending, each resolving the pattern's terms for itself.
    Partitions(Vec<u32>),
    /// A reverse read: first the object's home, for its referrer set.
    Reverse { home: u32 },
}

/// The partition holding the bucket of `iri` in `read`'s tenant.
fn owner_of(
    read: &ReadRequest,
    map: &PlacementMap,
    iri: &str,
    rule: &str,
) -> Result<u32, ErrorReply> {
    let bucket = iri_bucket(read.tenant, iri).ok_or_else(|| term_refused(0, rule))?;
    Ok(map.partition_of(bucket, Epoch::ZERO).get())
}

/// Code 25, refusing a read at latest level reaching `partitions`.
pub(crate) fn level_refused(partitions: &[u32]) -> ErrorReply {
    let lowest = partitions.first().copied().unwrap_or_default();
    let other = partitions.get(1).copied().unwrap_or_default();
    ErrorReply::new(
        ErrorCode::LevelRefused,
        format!(
            "a read at latest level reaches partitions {partitions:?}, of which it may reach one"
        ),
    )
    .with_detail(partitions_detail(lowest, other))
}

/// Code 23, refusing a read of the read-write transaction of `partition` reaching `other`.
fn cross_partition(partition: u32, other: u32) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::CrossPartition,
        format!("a read of the transaction of partition {partition} reaches partition {other}"),
    )
    .with_detail(partitions_detail(partition, other))
}

/// The route of a read-write transaction's read: its partition alone, whatever the pattern, or code 23.
fn own_partition(
    read: &ReadRequest,
    map: &PlacementMap,
    holders: &[u32],
) -> Result<ReadRoute, ErrorReply> {
    let own = read.partition;
    if let Some(subject) = &read.subject {
        let owner = owner_of(read, map, subject, "a subject names no bucket")?;
        if owner != own {
            return Err(cross_partition(own, owner));
        }
    } else if let Some(other) = holders.iter().find(|holder| **holder != own) {
        return Err(cross_partition(own, *other));
    }
    Ok(ReadRoute::Partitions(vec![own]))
}

/// Refuses a read at latest level that reaches a partition other than `partition` where one is bound, or more than one.
fn check_level(read: &ReadRequest, reached: &[u32]) -> Result<(), ErrorReply> {
    let other = reached
        .iter()
        .find(|partition| read.partition != 0 && **partition != read.partition);
    if read.level == ReadLevel::Latest && read.read_only && (reached.len() > 1 || other.is_some()) {
        let mut named: Vec<u32> = reached.to_vec();
        if read.partition != 0 && !named.contains(&read.partition) {
            named.push(read.partition);
            named.sort_unstable();
        }
        return Err(level_refused(&named));
    }
    Ok(())
}

impl ReadRoute {
    /// The route `read` takes through `map`, or its refusal: code 23 for a read-write transaction reaching another partition,
    /// code 25 for a read at latest level reaching more than it may.
    pub(crate) fn of(read: &ReadRequest, map: &PlacementMap) -> Result<Self, ErrorReply> {
        let holders: Vec<u32> = map
            .holders_of_run(Tenant::new(read.tenant), Epoch::ZERO)
            .into_iter()
            .map(|(holder, _)| holder.get())
            .collect();
        if read.is_read_write() {
            return own_partition(read, map, &holders);
        }
        let route = if let Some(subject) = &read.subject {
            Self::Partitions(vec![owner_of(
                read,
                map,
                subject,
                "a subject names no bucket",
            )?])
        } else if let Some(RequestTerm::Iri(object)) = &read.object {
            Self::Reverse {
                home: owner_of(read, map, object, "an object names no bucket")?,
            }
        } else {
            Self::Partitions(holders)
        };
        match &route {
            Self::Partitions(reached) => check_level(read, reached)?,
            // The home is reached whatever its referrer set holds, so a read at latest level bound elsewhere never asks it.
            Self::Reverse { home } => check_level(read, std::slice::from_ref(home))?,
        }
        Ok(route)
    }

    /// Refuses a reverse read at latest level whose home and members are `reached`.
    pub(crate) fn check_reached(read: &ReadRequest, reached: &[u32]) -> Result<(), ErrorReply> {
        check_level(read, reached)
    }
}
