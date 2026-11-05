/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_cluster_partition::BoundRead;
use ultragraphdb_core_keys::{Keyspace, key_prefix};
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply, RequestTermView, ScanRange, ScanView};
use ultragraphdb_store_quads::term_key;
use ultragraphdb_traits_store::Scan;

use crate::types::scan_read::ScanRead;

/// The bytes of a key's keyspace and tenant.
const TENANTED: usize = 9;

fn protocol(message: String) -> ErrorReply {
    ErrorReply::new(ErrorCode::Protocol, message)
}

/// Each range of `scan`, its keys copied, checked in order against what the role address serves and what a range may read
/// (`PROTOCOL.md` §13.13): partition `own`; a keyspace of lists, the dictionary or the referrer sets, no split part; a first
/// key naming a tenant; a past key at most the first key past that tenant; one key alone in the dictionary.
///
/// # Errors
///
/// Error 1 naming what it found.
pub(crate) fn scan_reads(scan: &ScanView<'_>, own: u32) -> Result<Vec<ScanRead>, ErrorReply> {
    scan.ranges()
        .enumerate()
        .map(|(index, range)| scan_read(index, &range, own))
        .collect()
}

fn scan_read(index: usize, range: &ScanRange<'_>, own: u32) -> Result<ScanRead, ErrorReply> {
    if range.partition() != own {
        return Err(protocol(format!(
            "this address hosts partition {own}, not {}",
            range.partition()
        )));
    }
    let (first, past) = (range.first(), range.past());
    let keyspace = keyspace_of(first)?;
    let tenant = first
        .get(1..TENANTED)
        .and_then(|bytes| <[u8; 8]>::try_from(bytes).ok())
        .map(u64::from_be_bytes)
        .ok_or_else(|| {
            protocol(format!(
                "range {index}: its first key is too short to name a tenant"
            ))
        })?;
    if keyspace == Keyspace::Dictionary {
        if past.strip_prefix(first) != Some(&[0u8][..]) {
            return Err(protocol(format!(
                "range {index} of the dictionary holds more than one key"
            )));
        }
    } else {
        let whole = Scan::prefix(key_prefix(keyspace, false, tenant, None, None));
        if !past.is_empty() && whole.end().is_some_and(|end| past > end) {
            return Err(protocol(format!(
                "range {index}: its past key lies beyond its tenant"
            )));
        }
    }
    Ok(ScanRead {
        keyspace,
        tenant,
        first: first.to_vec(),
        past: (!past.is_empty()).then(|| past.to_vec()),
    })
}

/// The keyspace `first` names, one the read kernel reads.
fn keyspace_of(first: &[u8]) -> Result<Keyspace, ErrorReply> {
    let byte = first.first().copied().unwrap_or_default();
    match Keyspace::from_byte(byte) {
        Some((keyspace, false))
            if keyspace.is_list()
                || keyspace == Keyspace::Dictionary
                || keyspace == Keyspace::Referrers =>
        {
            Ok(keyspace)
        }
        Some((keyspace, false)) => Err(protocol(format!(
            "a Scan reads no range of the keyspace {keyspace:?}"
        ))),
        Some((keyspace, true)) => Err(protocol(format!(
            "a Scan reads no split part of the keyspace {keyspace:?}"
        ))),
        None => Err(protocol(format!(
            "a Scan reads no range of the keyspace {byte:#04x}"
        ))),
    }
}

/// The IRIs `scan`'s term table names, in order.
///
/// # Errors
///
/// Error 1 naming an entry that is no IRI.
pub(crate) fn scan_terms(scan: &ScanView<'_>) -> Result<Vec<NamedNode>, ErrorReply> {
    let terms = scan.terms();
    (0..terms.len())
        .map(
            |index| match u32::try_from(index).ok().and_then(|at| terms.get(at)) {
                Some(RequestTermView::Iri(iri)) => NamedNode::new(iri)
                    .map_err(|error| protocol(format!("scan term {index}: {error}"))),
                _ => Err(protocol(format!("scan term {index} is no IRI"))),
            },
        )
        .collect()
}

/// `scan` against the transaction it names, `bound` (`PROTOCOL.md` §13.13): its version and principal, its tenant alone, no
/// referrer set, and its first ranges each holding one term's dictionary key alone.
///
/// # Errors
///
/// Error 1 naming the field or the range.
pub(crate) fn check_bound(
    scan: &ScanView<'_>,
    bound: &BoundRead,
    reads: &[ScanRead],
    terms: &[NamedNode],
) -> Result<(), ErrorReply> {
    if scan.read_version() != bound.read_version() {
        return Err(protocol(format!(
            "read_version {} is not the transaction's, {}",
            scan.read_version(),
            bound.read_version()
        )));
    }
    if scan.principal() != bound.principal() {
        return Err(protocol(format!(
            "principal {} is not the transaction's",
            scan.principal()
        )));
    }
    for (index, read) in reads.iter().enumerate() {
        if read.tenant != bound.tenant() {
            return Err(protocol(format!(
                "range {index} reads tenant {}, the transaction's being {}",
                read.tenant,
                bound.tenant()
            )));
        }
        if read.keyspace == Keyspace::Referrers {
            return Err(protocol(format!(
                "range {index} reads a referrer set, which a read-write transaction has no reason to ask"
            )));
        }
    }
    for (index, term) in terms.iter().enumerate() {
        let key = term_key(bound.tenant(), term.as_str());
        if reads.get(index).is_none_or(|read| read.first != key) {
            return Err(protocol(format!(
                "range {index} does not hold the dictionary key of scan term {index}"
            )));
        }
    }
    Ok(())
}
