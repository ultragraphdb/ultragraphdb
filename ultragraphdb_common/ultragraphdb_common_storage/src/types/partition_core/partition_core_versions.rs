/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_mapping::Declaration;
use ultragraphdb_protocol_wire::{Report, RequestTerm};
use ultragraphdb_store_apply::{ChangesetError, ChangesetErrorEnum};

use crate::types::partition_core::{FrontendAsk, PartitionCore};
use ultragraphdb_transactions_log::DurableStorage;

impl<S: DurableStorage> PartitionCore<'_, S> {
    /// Takes the catalog version a close announced, behind any that wait: one at a time and in order, so a run again never lands
    /// behind the version after it (`PROTOCOL.md` §11.12).
    pub(super) fn install(&mut self, contents: CatalogContents) {
        self.versions.push_back(contents);
        self.next_version();
    }

    /// Proposes the next version waiting, unless one is in the commit step or a build runs, for whose end it waits. Its execution
    /// reads, so a commit landing first aborts it, and it runs again against the data that commit left.
    pub(super) fn next_version(&mut self) {
        if !self.installing.is_empty() || self.host.partition().applier().rebuilding() {
            return;
        }
        let Some(contents) = self.versions.pop_front() else {
            return;
        };
        let now = self.now_us();
        match self.host.install_catalog(&contents, now) {
            Ok((ticket, refusals)) => {
                let _none = self.installing.insert(ticket, (contents, refusals));
            }
            // The pipeline stopped, or a build began since the look above, which this waits out.
            Err(error) if self.host.partition().applier().rebuilding() => {
                tracing::info!(%error, version = contents.version(), "a catalog version waits for a build");
                self.versions.push_front(contents);
            }
            Err(error) => {
                tracing::error!(%error, version = contents.version(), "a catalog version could not be installed");
            }
        }
    }

    /// The commit step decided the round of `contents`, of which the data refused `refusals`: durable, its outcome is reported;
    /// aborted, it runs again ahead of what waits.
    pub(super) fn version_decided(
        &mut self,
        contents: CatalogContents,
        refusals: &[ChangesetError],
        committed: bool,
    ) {
        if !committed {
            self.versions.push_front(contents);
            return;
        }
        for refusal in refusals {
            tracing::warn!(%refusal, version = contents.version(), "the data refuses a declaration");
        }
        let report = report_of(self.id.get(), &contents, refusals.first());
        // A frontend link gone is reported by the subscription, and the close delivered again checks again.
        drop(self.asks.unbounded_send(FrontendAsk::Report(report)));
    }
}

/// The report of `contents` by `partition`, `refusal` the first thing the data refused of it. A refusal names its predicate and
/// the subject at fault; one naming neither, its declaration unusable here, names the version's first declaration alone.
fn report_of(
    partition: u32,
    contents: &CatalogContents,
    refusal: Option<&ChangesetError>,
) -> Report {
    let (tenant, version) = (contents.tenant(), contents.version());
    let Some(refusal) = refusal else {
        return Report::passed(partition, tenant, version);
    };
    if let ChangesetErrorEnum::Contradicted {
        predicate, subject, ..
    } = &refusal.0
        && let Ok(report) = Report::refused(partition, tenant, version, predicate, term_of(subject))
    {
        return report;
    }
    let named = if let ChangesetErrorEnum::Rebuilding { predicate } = &refusal.0 {
        predicate.clone()
    } else {
        first_declared(contents)
    };
    Report::refused(
        partition,
        tenant,
        version,
        &named,
        RequestTerm::Iri(named.clone()),
    )
    // An IRI is a term any report holds.
    .unwrap_or_else(|_| Report::build_failed(partition, tenant, version, &named))
}

/// The first name `contents` declares, the empty name for none, which a refusal of a declaration cannot leave.
fn first_declared(contents: &CatalogContents) -> String {
    contents
        .declarations()
        .first()
        .map(|declaration| match declaration {
            Declaration::Predicate(held) => held.predicate().as_str().to_string(),
            Declaration::EntityType(held) => held.iri().as_str().to_string(),
            Declaration::Relation(held) => held.iri().as_str().to_string(),
        })
        .unwrap_or_default()
}

/// The term a contradiction names, in N-Triples: an IRI between angle brackets or a blank node; anything else as a plain literal.
fn term_of(subject: &str) -> RequestTerm {
    if let Some(iri) = subject
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
    {
        return RequestTerm::Iri(iri.to_string());
    }
    if let Some(label) = subject.strip_prefix("_:") {
        return RequestTerm::BlankNode(label.to_string());
    }
    RequestTerm::Literal {
        lexical: subject.to_string(),
        datatype: None,
    }
}
