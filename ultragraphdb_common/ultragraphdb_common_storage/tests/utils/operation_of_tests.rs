/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commit-time operation of a ConditionalWrite's operation, its terms read from the frame (`PROTOCOL.md` §6.7), through
//! what a partition's core makes of it: its graph and its properties.

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    Command, CommitOperation, ConditionalWrite, ErrorCode, ErrorReply, Link, OperationResult,
    RequestTerm, ScanAt, TermTable,
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

/// An upsert's links are the ones its frame names, to a blank node and from an IRI: the node they link is created with
/// them.
#[test]
fn test_an_upsert_links_the_nodes_it_names() -> TestResult {
    with_core("operation-links", RigSetup::default(), async |mut core| {
        declare_unique(&mut core, 1, &ex("id")).await?;
        let mut client = core.dial().await?;
        let mut terms = TermTable::new();
        let knows = terms.iri(&ex("knows"));
        let operation = CommitOperation::UpsertAndLink {
            graph: None,
            predicate: terms.iri(&ex("id")),
            value: terms.intern(RequestTerm::Literal {
                lexical: "u".to_string(),
                datatype: None,
            })?,
            properties: Vec::new(),
            links: vec![
                Link::to_node(
                    knows,
                    terms.intern(RequestTerm::BlankNode("b".to_string()))?,
                ),
                Link::from_node(knows, terms.iri(&ex("c"))),
            ],
        };
        let upsert = ConditionalWrite::new(TENANT, terms, Vec::new(), vec![operation])
            .with_principal(PRINCIPAL);
        assert!(created(one_frame(&mut client, &upsert).await?)?);
        let linked = [alone(term_key(TENANT, &ex("knows")))];
        let (found, refused) = scanned(&mut client, &plain(ScanAt::Latest, 0, 0, &linked)).await?;
        assert!(refused.is_none(), "{refused:?}");
        let entries: usize = found.iter().map(|batch| batch.entries.len()).sum();
        assert_eq!(entries, 1, "the links' predicate is held");
        Ok(())
    })
}

/// A counter whose `place` is the term at `index` of `terms`, every other place an IRI: its predicate for `"predicate"`,
/// its subject for `"subject"`, its graph for `"graph"`.
fn counter_with(terms: &mut TermTable, place: &str, index: u32) -> CommitOperation {
    let named = |table: &mut TermTable, name: &str, own: &str| {
        if place == name {
            index
        } else {
            table.iri(&ex(own))
        }
    };
    let subject = named(terms, "subject", "s");
    let predicate = named(terms, "predicate", "count");
    let graph = (place == "graph").then_some(index);
    CommitOperation::CounterAdd {
        graph,
        subject,
        predicate,
        delta: 1,
    }
}

/// A term of the wrong kind for its place is refused before anything runs, naming the place and the term's index: a
/// literal predicate or subject.
#[test]
fn test_a_term_of_the_wrong_kind_for_its_place_is_refused() -> TestResult {
    with_core("operation-kinds", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        for place in ["predicate", "subject"] {
            let mut terms = TermTable::new();
            let literal = terms.intern(RequestTerm::Literal {
                lexical: "x".to_string(),
                datatype: None,
            })?;
            let operation = counter_with(&mut terms, place, literal);
            let write = ConditionalWrite::new(TENANT, terms, Vec::new(), vec![operation])
                .with_principal(PRINCIPAL);
            let refused = one_frame(&mut client, &write)
                .await?
                .err()
                .context("a refusal")?;
            let expected = ErrorReply::protocol(&ProtocolError::field_value(
                Command::Operation.number(),
                place,
                literal.into(),
            ));
            assert_eq!(
                refused,
                (ErrorCode::Protocol, expected.message().to_string())
            );
        }
        Ok(())
    })
}

/// A counter in a graph its frame names by a blank node commits, the blank node naming the graph.
#[test]
fn test_an_operations_blank_graph_is_read_as_one() -> TestResult {
    with_core("operation-blank-graph", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let mut terms = TermTable::new();
        let blank = terms.intern(RequestTerm::BlankNode("g".to_string()))?;
        let operation = counter_with(&mut terms, "graph", blank);
        let write = ConditionalWrite::new(TENANT, terms, Vec::new(), vec![operation])
            .with_principal(PRINCIPAL);
        let answer = one_frame(&mut client, &write).await?;
        assert!(answer.is_ok(), "{answer:?}");
        Ok(())
    })
}
