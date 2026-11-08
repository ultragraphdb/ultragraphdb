/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A commit's reply names each node an operation found or created by its IRI or skolem IRI, through the reply's own term table
//! (`PROTOCOL.md` §§6.3, 7.3), against a core of `core_harness`.

use std::time::Duration;

use oxrdf::NamedNode;
use ultragraphdb_common_storage::CoreMessage;
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    BodyView, CommitOperation, CommitView, ConditionalWrite, RequestTerm, TermTable,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::types::partition_core::core_harness::{
    PRINCIPAL, Setup, TENANT, ex, next_frame, on_core, refusal_of,
};

/// How many times the test asks before the catalog version naming the unique predicate has taken effect.
const TRIES: u32 = 1_000;

/// A ConditionalWrite inserting, if absent, the node whose `unique` is `"x"`.
fn claim(unique: &str) -> TestResult<ConditionalWrite> {
    let mut terms = TermTable::new();
    let operation = CommitOperation::InsertIfAbsent {
        graph: None,
        predicate: terms.iri(unique),
        value: terms.intern(RequestTerm::Literal {
            lexical: "x".to_string(),
            datatype: None,
        })?,
        properties: Vec::new(),
    };
    Ok(ConditionalWrite::new(TENANT, terms, Vec::new(), vec![operation]).with_principal(PRINCIPAL))
}

/// An insert-if-absent's result names the node it created by its skolem IRI, under the tenant's skolem base, in the reply's term
/// table.
#[test]
fn test_a_created_node_is_named_by_its_skolem_iri() -> TestResult {
    on_core(
        "commit-node",
        Setup::default(),
        Address::parse("tcp://127.0.0.1:1")?,
        async |core| {
            let unique = ex("id");
            let declared = PredicateDeclaration::new(NamedNode::new(&unique)?).with_unique(true);
            let contents = CatalogContents::new(TENANT, 1, vec![Declaration::Predicate(declared)]);
            core.inbox.unbounded_send(CoreMessage::Catalog(contents))?;
            let mut client = core.dial().await?;
            for _ in 0..TRIES {
                client.send_only(&claim(&unique)?).await?;
                let reply = next_frame(&mut client).await?;
                if refusal_of(&reply)?.is_some() {
                    // The version has not taken effect yet.
                    compio::time::sleep(Duration::from_millis(2)).await;
                    continue;
                }
                let view = reply.view()?;
                let commit = CommitView::from_frame(&view)?;
                let result = commit.result(0).context("the operation's result")?;
                assert!(result.created(), "nothing held `x` before");
                let base =
                    format!("https://w3id.org/ultragraphdb/ns/tenant/{TENANT}/.well-known/genid/");
                let node = commit.node(0).context("the node's name")?;
                assert!(node.starts_with(&base), "{node}");
                return Ok(());
            }
            Err(TestError::new(
                "the unique predicate's version never took effect",
            ))
        },
    )
}
