/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod report_body_builder;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::report_kind::ReportKind;
use crate::types::request_term::RequestTerm;
use crate::types::term_table::TermTable;

/// A partition reports to the metadata group what it found at a catalog version's effective epoch
/// (`PROTOCOL.md` §13.19): a type change's check, passed or refused by the data, or an index build that failed. The
/// frontend answers with every field as sent.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `partition` | u32, at least 1 |
/// | 4 | `kind` | u8, a [`ReportKind`] |
/// | 8 | `namespace` | u64 |
/// | 16 | `version` | u64: the catalog version reported on, at least 1 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the term table's records: for a refusal, the predicate, then the term at fault unless it is the predicate's IRI; for a failed build, the predicate; none for a passed check |
/// | 1 | the term table's text |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    partition: u32,
    kind: ReportKind,
    namespace: u64,
    version: u64,
    terms: TermTable,
}

impl Report {
    /// `partition`'s report that catalog `version` of `namespace`'s type change passed its check.
    pub fn passed(partition: u32, namespace: u64, version: u64) -> Self {
        Self {
            partition,
            kind: ReportKind::Passed,
            namespace,
            version,
            terms: TermTable::new(),
        }
    }

    /// `partition`'s report that its data refused `version`'s change of `predicate`, `at_fault` the subject or
    /// object at fault. Refused as [`ProtocolError::bad_term`] when `at_fault` is a triple term, which the report
    /// cannot name, or a literal the term table refuses.
    pub fn refused(
        partition: u32,
        namespace: u64,
        version: u64,
        predicate: &str,
        at_fault: RequestTerm,
    ) -> Result<Self, ProtocolError> {
        let refusal = ProtocolError::bad_term(Command::Report.number(), 0, 1);
        if matches!(at_fault, RequestTerm::Triple { .. }) {
            return Err(refusal);
        }
        let mut report = Self::build_failed(partition, namespace, version, predicate);
        report.kind = ReportKind::Refused;
        let _at_fault: u32 = report.terms.intern(at_fault).map_err(|_| refusal)?;
        Ok(report)
    }

    /// `partition`'s report that `version`'s index build of `predicate` failed.
    pub fn build_failed(partition: u32, namespace: u64, version: u64, predicate: &str) -> Self {
        let mut report = Self::passed(partition, namespace, version);
        report.kind = ReportKind::BuildFailed;
        let _predicate: u32 = report.terms.iri(predicate);
        report
    }

    /// The reporting partition.
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// What the report says.
    pub fn kind(&self) -> ReportKind {
        self.kind
    }

    /// The namespace.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The catalog version reported on.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The term table: the predicate, and for a refusal the term at fault.
    pub fn terms(&self) -> &TermTable {
        &self.terms
    }
}
