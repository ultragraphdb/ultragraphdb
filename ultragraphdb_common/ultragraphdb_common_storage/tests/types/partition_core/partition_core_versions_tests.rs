/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The catalog versions a partition's core installs, one at a time and in order, and reports to the frontend
//! (`PROTOCOL.md` §§11.12, 13.19), against a core of `core_rig`.

use oxrdf::NamedNode;
use ultragraphdb_common_storage::CoreMessage;
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_mapping::{Cardinality, Declaration, PredicateDeclaration, VectorIndexSpec};
use ultragraphdb_protocol_wire::{Report, RequestTerm};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::partition_core::core_rig::{PARTITION, RigSetup, TENANT, with_core};
use crate::types::partition_core::core_writes::{Direct, edges, ex};

/// The declaration of predicate `name`, with no more than its name.
fn declared(name: &str) -> TestResult<Declaration> {
    Ok(Declaration::Predicate(PredicateDeclaration::new(
        NamedNode::new(ex(name))?,
    )))
}

/// Version `version` of the tenant's catalog, declaring `declarations`.
fn version(version: u64, declarations: Vec<Declaration>) -> CoreMessage {
    CoreMessage::Catalog(CatalogContents::new(TENANT, version, declarations))
}

/// A version a close announced is installed, and its outcome reported to the frontend: passed, the data refusing nothing.
#[test]
fn test_a_version_is_installed_and_reported_passed() -> TestResult {
    with_core("versions-passed", RigSetup::default(), async |mut core| {
        core.inbox
            .unbounded_send(version(1, vec![declared("q")?]))?;
        assert_eq!(core.report().await?, Report::passed(PARTITION, TENANT, 1));
        Ok(())
    })
}

/// Versions announced together are installed one at a time, in order, each reported.
#[test]
fn test_versions_announced_together_are_installed_in_order() -> TestResult {
    with_core(
        "versions-in-order",
        RigSetup::default(),
        async |mut core| {
            core.inbox
                .unbounded_send(version(1, vec![declared("q")?]))?;
            core.inbox
                .unbounded_send(version(2, vec![declared("q")?, declared("r")?]))?;
            assert_eq!(core.report().await?, Report::passed(PARTITION, TENANT, 1));
            assert_eq!(core.report().await?, Report::passed(PARTITION, TENANT, 2));
            Ok(())
        },
    )
}

/// A declaration unusable here, refused naming neither a predicate nor a subject at fault, is reported naming the version's
/// first declaration alone (`PROTOCOL.md` §13.19).
#[test]
fn test_a_declaration_unusable_here_is_reported_by_the_first_declared_name() -> TestResult {
    with_core(
        "versions-unusable",
        RigSetup::default(),
        async |mut core| {
            // A vector index declared without its dimension, which no partition can build.
            let unusable = PredicateDeclaration::new(NamedNode::new(ex("v"))?)
                .with_vector_index(VectorIndexSpec::new("hnsw", Vec::new()));
            let declarations = vec![declared("first")?, Declaration::Predicate(unusable)];
            core.inbox.unbounded_send(version(1, declarations))?;
            let first = ex("first");
            let expected = Report::refused(
                PARTITION,
                TENANT,
                1,
                &first,
                RequestTerm::Iri(first.clone()),
            )?;
            assert_eq!(core.report().await?, expected);
            Ok(())
        },
    )
}

/// Version `version` declaring `<q>` single-valued: its execution reads `<q>`'s lists, so a commit writing `<q>` landing first
/// aborts it.
fn single_q(version_number: u64) -> TestResult<CoreMessage> {
    let single =
        PredicateDeclaration::new(NamedNode::new(ex("q"))?).with_cardinality(Cardinality::Single);
    Ok(version(
        version_number,
        vec![Declaration::Predicate(single)],
    ))
}

/// A version a commit landing first aborted runs again ahead of the version after it, which never lands before it
/// (`PROTOCOL.md` §11.12).
#[test]
fn test_an_aborted_version_runs_again_ahead_of_the_next() -> TestResult {
    with_core("versions-aborted", RigSetup::default(), async |mut core| {
        let mut direct = Direct::open(&core.inbox, 1_000, 1 << 20)?;
        // The write, then both versions, all handed over before the core takes any: the write lands first.
        let _answer = direct.send(1, &edges(TENANT, &[("s", "q", "o")]))?;
        core.inbox.unbounded_send(single_q(1)?)?;
        core.inbox.unbounded_send(single_q(2)?)?;
        let reported: Vec<u64> = vec![
            core.report().await?.version(),
            core.report().await?.version(),
        ];
        assert_eq!(reported, vec![1, 2]);
        Ok(())
    })
}
