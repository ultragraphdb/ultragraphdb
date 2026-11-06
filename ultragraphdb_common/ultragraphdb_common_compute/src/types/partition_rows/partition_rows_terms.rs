/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::Graph;
use ultragraphdb_protocol_wire::{
    EntriesView, ErrorReply, RequestTerm, ResultTerm, ResumeKey, ResumeRow, ScanRange, TermTable,
};

use super::{PartitionRows, RANGES_PER_SCAN};
use crate::types::names::Names;
use crate::types::page::RowView;
use crate::types::read_fault::ReadFault;
use crate::types::read_request::{GraphSlot, ReadRequest};
use crate::types::row_terms::{RowIds, id_term, reifier_term, request_term, result_term};
use crate::types::scan_stream::ScanStream;
use crate::types::wanted_names::WantedNames;
use crate::utils::keys::{hex, past_alone};
use crate::utils::refusals::unavailable;

/// The dictionary bytes a partition's read takes before it lets the merge go on: no Result frame holds more.
const NAME_BYTES: usize = 1 << 20;

/// A row's unbound positions, by their terms, each named by the partition that answered it.
#[derive(Debug, Default)]
pub(crate) struct RowCells {
    pub(crate) graph: Option<ResultTerm>,
    pub(crate) subject: Option<ResultTerm>,
    pub(crate) predicate: Option<ResultTerm>,
    pub(crate) object: Option<ResultTerm>,
    pub(crate) reifier: Option<ResultTerm>,
}

impl RowCells {
    /// The cells in the columns of `read`: its unbound slots in slot order, then the reifier.
    pub(crate) fn columns(&self, read: &ReadRequest) -> Vec<Option<&ResultTerm>> {
        let mut cells = Vec::with_capacity(5);
        if read.graph == GraphSlot::Unbound {
            cells.push(self.graph.as_ref());
        }
        if read.subject.is_none() {
            cells.push(self.subject.as_ref());
        }
        if read.predicate.is_none() {
            cells.push(self.predicate.as_ref());
        }
        if read.object.is_none() {
            cells.push(self.object.as_ref());
        }
        cells.push(self.reifier.as_ref());
        cells
    }
}

/// The dictionary keys of what `row` names in an unbound position of `read`, or the row's refusal.
fn wants(
    read: &ReadRequest,
    row: &RowView<'_>,
    names: &Names,
    keys: &mut WantedNames,
) -> Option<()> {
    let ids = RowIds::of(row)?;
    if read.graph == GraphSlot::Unbound
        && let Graph::Named(id) = ids.graph
    {
        names.want_id(id, keys);
    }
    if read.subject.is_none()
        && let Some(subject) = &ids.subject
    {
        names.want_term(subject, keys);
    }
    if read.predicate.is_none()
        && let Some(predicate) = ids.predicate
    {
        names.want_id(predicate, keys);
    }
    if read.object.is_none() {
        names.want_term(&ids.object, keys);
    }
    if let Some(statement) = ids.statement {
        names.want_id(statement, keys);
    }
    Some(())
}

impl PartitionRows {
    /// The dangling refusal of the list of `key`: code 17, the message naming the key holding the ID or hash.
    fn dangling(&self, key: &[u8]) -> ErrorReply {
        unavailable(format!(
            "the list {} of partition {} names a dictionary entry the partition lacks",
            hex(key),
            self.partition()
        ))
    }

    /// Whether every name `row` wants is held.
    fn has_names(&self, row: &RowView<'_>) -> bool {
        let mut still = WantedNames::default();
        wants(&self.shared.read, row, &self.names, &mut still).is_some() && still.is_empty()
    }

    /// Gives the page's next row its names, and as many rows after it as a read of the names reaches: one Scan of one-key ranges,
    /// of the rows the read may still return, given up once it has held about a Result frame's worth of text. So a read holds the
    /// names of the rows it is about to return, whatever a frame's rows name and however little its limit asks. A name the
    /// dictionary answers no entry for refuses the read, naming the list.
    pub(super) async fn name_ahead(&mut self) -> Result<(), ErrorReply> {
        if self.page.next_is_named() {
            return Ok(());
        }
        self.names.shed();
        let room = match self.shared.remaining.get() {
            0 => usize::MAX,
            left => left as usize,
        };
        let mut wanted = WantedNames::default();
        let mut rows = 0usize;
        for row in self.page.rows_unnamed().take(room) {
            if wants(&self.shared.read, &row, &self.names, &mut wanted).is_none() {
                if rows == 0 {
                    return Err(self.dangling(row.key));
                }
                break;
            }
            rows = rows.saturating_add(1);
            if wanted.len() >= RANGES_PER_SCAN {
                break;
            }
        }
        if !wanted.is_empty() {
            self.read_names(&wanted).await?;
        }
        let named = self
            .page
            .rows_unnamed()
            .take(rows)
            .take_while(|row| self.has_names(row))
            .count();
        if named == 0
            && let Some(row) = self.page.rows_unnamed().next()
        {
            return Err(self.dangling(row.key));
        }
        self.page.named_more(named);
        Ok(())
    }

    /// Reads the texts of `wanted`, as far as the page's next row's and [`NAME_BYTES`] take it.
    async fn read_names(&mut self, wanted: &WantedNames) -> Result<(), ErrorReply> {
        let mut scan = self.scan(0);
        for key in wanted.keys() {
            scan.push_range(ScanRange::new(self.partition(), key, &past_alone(key)));
        }
        let mut stream = ScanStream::open(&self.link, &scan)?;
        let mut bytes = 0usize;
        loop {
            let names = &mut self.names;
            let (facts, ()) = stream
                .next(|view| take_names(view, names, &mut bytes))
                .await?;
            self.note_version(facts);
            if facts.last {
                return Ok(());
            }
            let next_named = self
                .page
                .rows_unnamed()
                .next()
                .is_none_or(|row| self.has_names(&row));
            if bytes >= NAME_BYTES && next_named {
                return Ok(());
            }
        }
    }

    /// The next row's cells.
    pub(crate) fn cells(&self) -> Result<RowCells, ErrorReply> {
        let read = &self.shared.read;
        let row = self
            .page
            .peek()
            .ok_or_else(|| unavailable("a merge asked a partition with no row"))?;
        let ids = RowIds::of(&row).ok_or_else(|| self.dangling(row.key))?;
        let term_of = |term: &ultragraphdb_core_terms::EncodedTerm| {
            result_term(read.tenant, term, &self.names).ok_or_else(|| self.dangling(row.key))
        };
        let mut cells = RowCells::default();
        if read.graph == GraphSlot::Unbound
            && let Graph::Named(id) = ids.graph
        {
            cells.graph =
                Some(id_term(read.tenant, id, &self.names).ok_or_else(|| self.dangling(row.key))?);
        }
        if read.subject.is_none()
            && let Some(subject) = &ids.subject
        {
            cells.subject = Some(term_of(subject)?);
        }
        if read.predicate.is_none()
            && let Some(predicate) = ids.predicate
        {
            cells.predicate = Some(
                id_term(read.tenant, predicate, &self.names)
                    .ok_or_else(|| self.dangling(row.key))?,
            );
        }
        if read.object.is_none() {
            cells.object = Some(term_of(&ids.object)?);
        }
        if let Some(statement) = ids.statement {
            cells.reifier =
                Some(reifier_term(statement, &self.names).ok_or_else(|| self.dangling(row.key))?);
        }
        Ok(cells)
    }

    /// The resume key continuing the read after the next row; every position by its text, bound ones from the request.
    pub(crate) fn resume_key_of_next(&self) -> Result<Vec<u8>, ErrorReply> {
        let read = &self.shared.read;
        let row = self
            .page
            .peek()
            .ok_or_else(|| unavailable("a resume key of no row"))?;
        let ids = RowIds::of(&row).ok_or_else(|| self.dangling(row.key))?;
        let lost = || self.dangling(row.key);
        let id_text = |id| self.names.of_id(id).map(str::to_string).ok_or_else(lost);
        let mut table = TermTable::new();
        let graph = match (&read.graph, ids.graph) {
            (GraphSlot::Named(iri), _) => Some(table.iri(iri)),
            (GraphSlot::Unbound | GraphSlot::Default, Graph::Default) => None,
            (GraphSlot::Unbound | GraphSlot::Default, Graph::Named(id)) => {
                Some(table.iri(&id_text(id)?))
            }
        };
        let subject = match (&read.subject, &ids.subject) {
            (Some(iri), _) => iri.clone(),
            (None, Some(subject)) => match request_term(subject, &self.names) {
                Some(RequestTerm::Iri(iri)) => iri,
                _ => return Err(lost()),
            },
            (None, None) => return Err(lost()),
        };
        let subject = table.iri(&subject);
        let predicate = match (&read.predicate, ids.predicate) {
            (Some(iri), _) => iri.clone(),
            (None, Some(id)) => id_text(id)?,
            (None, None) => return Err(lost()),
        };
        let predicate = table.iri(&predicate);
        let object = match &read.object {
            Some(term) => term.clone(),
            None => request_term(&ids.object, &self.names).ok_or_else(lost)?,
        };
        let object = table
            .intern(object)
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        let reifier = match ids.statement {
            Some(id) => Some(table.iri(&id_text(id)?)),
            None => None,
        };
        let key = ResumeKey::new(
            read.pattern_hash,
            read.scope(),
            table,
            ResumeRow::new(graph, subject, predicate, object, reifier),
        );
        Ok(key.encode())
    }
}

/// The dictionary's texts one frame holds, into `names`, counted into `bytes`.
fn take_names(
    view: &EntriesView<'_>,
    names: &mut Names,
    bytes: &mut usize,
) -> Result<(), ReadFault> {
    for entry in view.entries().iter() {
        let value = entry.value().unwrap_or_default();
        let text = std::str::from_utf8(value).map_err(|error| {
            ReadFault::Stored(format!(
                "the dictionary entry {} holds no text: {error}",
                hex(entry.key())
            ))
        })?;
        *bytes = bytes.saturating_add(value.len());
        names.hold(entry.key().to_vec(), text.to_string());
    }
    Ok(())
}
