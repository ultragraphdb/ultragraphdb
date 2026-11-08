/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A pattern's literal object as the store keys it (`PROTOCOL.md` §10, §11.12): by value, by its known datatype, or by the system
//! ID its partition's dictionary gives its datatype; a vector by no reverse key.

use oxrdf::vocab::xsd;
use oxrdf::{Literal, NamedNode};
use ultragraphdb_common_compute::read_times_of;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey};
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR, TermError};
use ultragraphdb_protocol_wire::RequestTerm;
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::played_answers::{Pattern, answer_on, snapshot};
use crate::utils::played_cluster::{Played, TENANT, id, list_of, run};

const S: &str = "http://ex.org/s";
const P: &str = "http://ex.org/p";
const DATATYPE: &str = "http://ex.org/datatype";

/// The store's encoding of `literal`, its datatype by `datatype` where the store names it by ID.
fn encoded(literal: &Literal, datatype: SystemId) -> TestResult<EncodedTerm> {
    Ok(EncodedTerm::literal(literal.as_ref(), |_| Ok::<_, TermError>(datatype))?.into_term())
}

/// The request term of `literal`: its lexical form, and its datatype unless `xsd:string`.
fn term_of(literal: &Literal) -> RequestTerm {
    RequestTerm::Literal {
        lexical: literal.value().to_string(),
        datatype: (literal.datatype() != xsd::STRING)
            .then(|| literal.datatype().as_str().to_string()),
    }
}

/// The rows of `<s> <p> ?o`'s reverse, `?s <p> literal`, over one partition holding `<s> <p> literal` and the datatype's term, and
/// the keys of the lists it was asked.
async fn read_literal(literal: &Literal) -> TestResult<(Vec<Vec<Option<String>>>, Vec<Vec<u8>>)> {
    let played = Played::start(1, read_times_of(None, None)).await?;
    let partition = played.partition(1)?;
    let (s, p, datatype) = (id(1)?, id(2)?, id(3)?);
    partition.term(S, s);
    partition.term(P, p);
    partition.term(DATATYPE, datatype);
    partition.quads(Graph::Default, s, p, &[encoded(literal, datatype)?]);
    let mut client = played.client().await?;
    let read = snapshot(&Pattern::of(None, Some(P), Some(term_of(literal))))?;
    let rows = answer_on(&mut client, &read).await?.whole()?;
    let lists = partition
        .scans()
        .into_iter()
        .flat_map(|scan| scan.ranges.into_iter().map(|range| range.first))
        .filter(|first| {
            QuadKey::parse(first).is_ok_and(|(key, _)| {
                matches!(key.keyspace(), Keyspace::Forward | Keyspace::Reverse)
            })
        })
        .collect();
    Ok((rows, lists))
}

/// The one row `?s <p> literal` answers: the subject, and no reifier.
fn the_subject() -> Vec<Vec<Option<String>>> {
    vec![vec![Some(S.to_string()), None]]
}

/// A plain literal, an `xsd:string`, is keyed by value: its reverse list holds its subject.
#[test]
fn test_a_plain_literal_object_reads_its_reverse_list() -> TestResult {
    run(async {
        let literal = Literal::new_simple_literal("ann");
        let (rows, lists) = read_literal(&literal).await?;
        assert_eq!(rows, the_subject());
        let reverse = QuadKey::Reverse {
            tenant: TENANT,
            graph: Graph::Default,
            predicate: id(2)?,
            object: encoded(&literal, id(3)?)?,
        };
        assert_eq!(lists, vec![reverse.to_bytes()]);
        Ok(())
    })
}

/// A literal of a known datatype is keyed by value and that datatype's code, with no dictionary entry asked for it.
#[test]
fn test_a_literal_of_a_known_datatype_reads_its_rows() -> TestResult {
    run(async {
        let literal = Literal::new_typed_literal("5", xsd::INTEGER);
        let (rows, _) = read_literal(&literal).await?;
        assert_eq!(rows, the_subject());
        Ok(())
    })
}

/// A literal of another datatype is keyed by the system ID the partition's dictionary gives that datatype's IRI.
#[test]
fn test_a_literal_of_a_datatype_named_by_id_reads_its_rows() -> TestResult {
    run(async {
        let literal = Literal::new_typed_literal("x", NamedNode::new(DATATYPE)?);
        let (rows, _) = read_literal(&literal).await?;
        assert_eq!(rows, the_subject());
        Ok(())
    })
}

/// A vector, which no reverse key names, is read from its predicate's forward lists, the rows holding it taken.
#[test]
fn test_a_vector_object_reads_the_forward_lists() -> TestResult {
    run(async {
        let played = Played::start(1, read_times_of(None, None)).await?;
        let partition = played.partition(1)?;
        let (s, other, p) = (id(1)?, id(4)?, id(2)?);
        partition.term(S, s);
        partition.term("http://ex.org/other", other);
        partition.term(P, p);
        let vector = Literal::new_typed_literal("[1, -2.5, 3]", NamedNode::new(FLOAT32_VECTOR)?);
        let held = encoded(&vector, p)?;
        let elsewhere = encoded(
            &Literal::new_typed_literal("[1, 2, 3]", NamedNode::new(FLOAT32_VECTOR)?),
            p,
        )?;
        for (subject, object) in [(s, &held), (other, &elsewhere)] {
            let forward = QuadKey::Forward {
                tenant: TENANT,
                graph: Graph::Default,
                predicate: p,
                subject,
            };
            partition.put(forward.to_bytes(), list_of(std::slice::from_ref(object)));
        }
        let mut client = played.client().await?;
        let read = snapshot(&Pattern::of(None, Some(P), Some(term_of(&vector))))?;
        let rows = answer_on(&mut client, &read).await?.whole()?;
        assert_eq!(rows, the_subject());
        Ok(())
    })
}
