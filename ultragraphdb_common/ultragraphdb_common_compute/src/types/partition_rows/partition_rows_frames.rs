/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Keyspace, QuadKey};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{EntriesView, ObjectRecord, ObjectRecords};
use ultragraphdb_store_quads::StatementRecord;

use super::row_filter::RowFilter;
use super::skip::Skip;
use crate::types::page::Page;
use crate::types::read_fault::ReadFault;

/// A frame's last key and that key's last object.
pub(super) type FrameTail = (Vec<u8>, Vec<u8>);

/// What a stored value that does not read says of a read.
fn stored(error: impl std::fmt::Display) -> ReadFault {
    ReadFault::Stored(error.to_string())
}

/// What an object record running past its value says of its frame (`PROTOCOL.md` §13.14).
fn broken(error: impl std::fmt::Display) -> ReadFault {
    ReadFault::Frame(error.to_string())
}

/// The rows of one Entries frame into `page`, as `filter` and `skip` read them: the frame's last key and that key's last object.
pub(super) fn take_rows(
    view: &EntriesView<'_>,
    page: &mut Page,
    filter: &RowFilter,
    skip: Option<&Skip>,
) -> Result<Option<FrameTail>, ReadFault> {
    let mut last = None;
    for entry in view.entries().iter() {
        let key = entry.key();
        let (parsed, _) = QuadKey::parse(key).map_err(stored)?;
        let value = entry.value().unwrap_or_default();
        let keyspace = parsed.keyspace();
        page.add_entry(key, parsed);
        let mut object: &[u8] = &[];
        if keyspace == Keyspace::Statements {
            take_statement(key, value, page, filter, skip)?;
        } else {
            let records = ObjectRecords::read(value).map_err(broken)?;
            for record in records.iter() {
                take_record(&record, key, page, filter, skip)?;
                object = record.encoding();
            }
        }
        last = Some((key, object));
    }
    Ok(last.map(|(key, object)| (key.to_vec(), object.to_vec())))
}

/// The rows of one object record: one, or one per statement ID, those a resumed read has passed dropped.
fn take_record(
    record: &ObjectRecord<'_>,
    key: &[u8],
    page: &mut Page,
    filter: &RowFilter,
    skip: Option<&Skip>,
) -> Result<(), ReadFault> {
    let encoding = record.encoding();
    if filter
        .object
        .as_deref()
        .is_some_and(|object| object != encoding)
    {
        return Ok(());
    }
    let passed = match skip {
        Some(Skip::Object {
            key: list,
            encoding: at,
            statement,
        }) if list == key && at == encoding => Some(*statement),
        Some(Skip::Object { .. } | Skip::Property { .. }) | None => None,
    };
    if record.statement_count() == 0 {
        if passed.is_none() {
            page.add_row(encoding, None, None);
        }
        return Ok(());
    }
    for id in record.statement_ids() {
        let statement = SystemId::decode(&id).map_err(stored)?;
        let dropped = match passed {
            Some(Some(last)) => statement <= last,
            Some(None) => true,
            None => false,
        };
        if !dropped {
            page.add_row(encoding, Some(statement), None);
        }
    }
    Ok(())
}

/// One row per property of the statement's record `value` that the filter takes, in the order of predicate ID then encoding, those
/// a resumed read has passed dropped: a property the read stopped at may since have been removed, and that order passes it still.
fn take_statement(
    key: &[u8],
    value: &[u8],
    page: &mut Page,
    filter: &RowFilter,
    skip: Option<&Skip>,
) -> Result<(), ReadFault> {
    let record = StatementRecord::decode(key, value).map_err(stored)?;
    let mut properties: Vec<&(SystemId, EncodedTerm)> = record.properties().iter().collect();
    properties.sort_by(|a, b| (a.0, a.1.as_bytes()).cmp(&(b.0, b.1.as_bytes())));
    let from = match skip {
        Some(Skip::Property {
            predicate,
            value: at,
        }) => properties
            .partition_point(|held| (held.0, held.1.as_bytes()) <= (*predicate, at.as_bytes())),
        Some(Skip::Object { .. }) | None => 0,
    };
    for (predicate, term) in properties.iter().skip(from).copied() {
        let taken = filter.property.is_none_or(|bound| bound == *predicate)
            && filter
                .object
                .as_deref()
                .is_none_or(|object| object == term.as_bytes());
        if taken {
            page.add_row(term.as_bytes(), None, Some(*predicate));
        }
    }
    Ok(())
}

/// The predicates one frame of a predicates' list names, into `found`.
pub(super) fn predicates_of(
    view: &EntriesView<'_>,
    found: &mut Vec<SystemId>,
) -> Result<(), ReadFault> {
    for entry in view.entries().iter() {
        let records = ObjectRecords::read(entry.value().unwrap_or_default()).map_err(broken)?;
        for record in records.iter() {
            let id = EncodedTerm::from_bytes(record.encoding())
                .ok()
                .and_then(|term| term.as_id())
                .ok_or_else(|| stored("a predicates' list holds an object naming no system ID"))?;
            found.push(id);
        }
    }
    Ok(())
}
