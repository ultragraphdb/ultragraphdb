/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commit-time operation of a ConditionalWrite's operation, its terms read from the frame (`PROTOCOL.md` §6.7), through
//! what a partition's core makes of it: its graph and its properties.

use ultragraphdb_protocol_wire::{
    CommitOperation, ConditionalWrite, OperationResult, RequestTerm, ScanAt, TermTable,
};
use ultragraphdb_store_quads::term_key;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::types::partition_core::core_rig::{PRINCIPAL, RigSetup, TENANT, with_core};
use crate::types::partition_core::core_writes::{
    alone, declare_unique, ex, one_frame, plain, scanned,
};

/// A ConditionalWrite inserting, if absent, the node whose `id` is `value` in `graph` (the default graph for `None`), with
/// `properties` of names under `http://ex.org/` and literal values.
fn claim(
    graph: Option<&str>,
    value: &str,
    properties: &[(&str, &str)],
) -> TestResult<ConditionalWrite> {
    let mut terms = TermTable::new();
    let literal = |lexical: &str| RequestTerm::Literal {
        lexical: lexical.to_string(),
        datatype: None,
    };
    let operation = CommitOperation::InsertIfAbsent {
        graph: graph.map(|name| terms.iri(&ex(name))),
        predicate: terms.iri(&ex("id")),
        value: terms.intern(literal(value))?,
        properties: properties
            .iter()
            .map(|(name, held)| Ok((terms.iri(&ex(name)), terms.intern(literal(held))?)))
            .collect::<TestResult<Vec<_>>>()?,
    };
    Ok(ConditionalWrite::new(TENANT, terms, Vec::new(), vec![operation]).with_principal(PRINCIPAL))
}

/// Whether the node the one result of `answer` names was created.
fn created(
    answer: Result<
        crate::types::partition_core::core_writes::Committed,
        (ultragraphdb_protocol_wire::ErrorCode, String),
    >,
) -> TestResult<bool> {
    let committed = answer.map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
    let result: &OperationResult = committed
        .results
        .first()
        .context("the operation's result")?;
    Ok(result.created())
}

/// An operation's graph is the one its frame names: a node claimed in a named graph is absent from the default graph.
#[test]
fn test_an_operation_claims_in_the_graph_it_names() -> TestResult {
    with_core("operation-graph", RigSetup::default(), async |mut core| {
        declare_unique(&mut core, 1, &ex("id")).await?;
        let mut client = core.dial().await?;
        assert!(created(
            one_frame(&mut client, &claim(Some("g"), "x", &[])?).await?
        )?);
        assert!(
            created(one_frame(&mut client, &claim(None, "x", &[])?).await?)?,
            "absent from the default graph"
        );
        assert!(
            !created(one_frame(&mut client, &claim(None, "x", &[])?).await?)?,
            "present once claimed there"
        );
        Ok(())
    })
}

/// An operation's properties are the ones its frame names, each written with the node it creates.
#[test]
fn test_an_operation_writes_the_properties_it_names() -> TestResult {
    with_core(
        "operation-properties",
        RigSetup::default(),
        async |mut core| {
            declare_unique(&mut core, 1, &ex("id")).await?;
            let mut client = core.dial().await?;
            assert!(created(
                one_frame(&mut client, &claim(None, "x", &[("name", "n")])?).await?
            )?);
            let term = [alone(term_key(TENANT, &ex("name")))];
            let (found, refused) =
                scanned(&mut client, &plain(ScanAt::Latest, 0, 0, &term)).await?;
            assert!(refused.is_none(), "{refused:?}");
            let entries: usize = found.iter().map(|batch| batch.entries.len()).sum();
            assert_eq!(entries, 1, "the property's predicate is held");
            Ok(())
        },
    )
}
