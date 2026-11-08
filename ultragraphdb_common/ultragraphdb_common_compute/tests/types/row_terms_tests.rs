/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! How a row's terms come back (`PROTOCOL.md` §13.12, §10.3): a blank node by its skolem IRI and tagged so, a reifier as a blank
//! node, a literal of a datatype the store names by ID by the hash of that datatype's IRI, and a resume key's last row by its
//! terms' texts (§11.8).

use oxrdf::Literal;
use ultragraphdb_common_compute::read_times_of;
use ultragraphdb_core_ids::{Sequence, SystemId};
use ultragraphdb_core_terms::{EncodedTerm, Hash128};
use ultragraphdb_protocol_wire::{ReadLevel, RequestTermView, ResumeKeyView};
use ultragraphdb_store_quads::is_skolem;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::played_reads::played_cluster::{PlayedCluster, run};
use crate::played_reads::played_data::{
    first_bucket, forward_key, id_in, ids_list, list_of, literal_held, name, skolem_iri,
    tenant_over, typed,
};
use crate::played_reads::read_answer::{Cell, Pattern, ReadAnswer, answer_on};

const SUBJECT: &str = "http://ex.org/s";
const PREDICATE: &str = "http://ex.org/p";

/// A cluster of one partition and its tenant, the predicate named by the ID of sequence 2 in the run's first bucket.
async fn cluster() -> TestResult<(PlayedCluster, u64, SystemId)> {
    let cluster = PlayedCluster::start(1, read_times_of(None, None)).await?;
    let tenant = tenant_over(1, 1)?;
    let predicate = id_in(first_bucket(tenant), 2)?;
    name(cluster.partition(1)?, tenant, PREDICATE, predicate);
    Ok((cluster, tenant, predicate))
}

/// The cells of column `column`, every row's.
fn cells(answer: &ReadAnswer, column: usize) -> TestResult<Vec<Cell>> {
    answer
        .rows()
        .iter()
        .map(|row| row.get(column).and_then(Clone::clone).context("a cell"))
        .collect()
}

/// A subject named by a skolem IRI of the tenant is a blank node; one named by any other IRI is not.
#[test]
fn test_a_skolem_subject_comes_back_a_blank_node() -> TestResult {
    run(async {
        let (cluster, tenant, predicate) = cluster().await?;
        let part = cluster.partition(1)?;
        let bucket = first_bucket(tenant);
        let skolem = skolem_iri(tenant, bucket, 0xabc);
        assert!(
            is_skolem(tenant, &skolem),
            "{skolem} is the tenant's skolem form"
        );
        let subjects = [
            (SUBJECT, id_in(bucket, 10)?),
            (skolem.as_str(), id_in(bucket, 11)?),
        ];
        let object = id_in(bucket, 20)?;
        name(part, tenant, "http://ex.org/o", object);
        for (iri, id) in subjects {
            name(part, tenant, iri, id);
            part.put(forward_key(tenant, predicate, id), ids_list(&[object]));
        }
        let read =
            Pattern::of(None, Some(PREDICATE), None).read(tenant, true, ReadLevel::Latest)?;
        let answer = answer_on(&mut cluster.client().await?, &read).await?;
        let named = cells(&answer, 0)?;
        let texts: Vec<&str> = named.iter().map(|cell| cell.text.as_str()).collect();
        assert_eq!(texts, [SUBJECT, skolem.as_str()]);
        let blank: Vec<bool> = named.iter().map(|cell| cell.blank).collect();
        assert_eq!(blank, [false, true]);
        Ok(())
    })
}

/// An object held under a statement ID comes back with its reifier, a blank node named by the statement's skolem IRI.
#[test]
fn test_a_reifier_comes_back_a_blank_node() -> TestResult {
    run(async {
        let (cluster, tenant, predicate) = cluster().await?;
        let part = cluster.partition(1)?;
        let bucket = first_bucket(tenant);
        let subject = id_in(bucket, 1)?;
        let object = id_in(bucket, 20)?;
        let statement = SystemId::statement(subject, Sequence::new(30)?);
        let reifier = skolem_iri(tenant, bucket, 0xdef);
        name(part, tenant, SUBJECT, subject);
        name(part, tenant, "http://ex.org/o", object);
        name(part, tenant, &reifier, statement);
        part.put(
            forward_key(tenant, predicate, subject),
            list_of(&[(EncodedTerm::id(object), vec![statement])]),
        );
        let read = Pattern::of(Some(SUBJECT), Some(PREDICATE), None).read(
            tenant,
            true,
            ReadLevel::Latest,
        )?;
        let answer = answer_on(&mut cluster.client().await?, &read).await?;
        let reifiers = cells(&answer, 1)?;
        let first = reifiers.first().context("the row's reifier")?;
        assert_eq!(first.text, reifier);
        assert!(first.blank, "a reifier is a blank node");
        Ok(())
    })
}

/// A literal whose datatype the store names by ID comes back with that datatype by the hash of its IRI, its IRI its annotation
/// (`PROTOCOL.md` §10.3).
#[test]
fn test_a_datatype_held_by_id_comes_back_by_its_iris_hash() -> TestResult {
    run(async {
        let (cluster, tenant, predicate) = cluster().await?;
        let part = cluster.partition(1)?;
        let bucket = first_bucket(tenant);
        let subject = id_in(bucket, 1)?;
        let datatype = id_in(bucket, 40)?;
        let datatype_iri = "http://ex.org/datatype";
        name(part, tenant, SUBJECT, subject);
        name(part, tenant, datatype_iri, datatype);
        let held = literal_held(part, tenant, &typed("v", datatype_iri), Some(datatype))?;
        part.put(
            forward_key(tenant, predicate, subject),
            list_of(&[(held.clone(), Vec::new())]),
        );
        let read = Pattern::of(Some(SUBJECT), Some(PREDICATE), None).read(
            tenant,
            true,
            ReadLevel::Latest,
        )?;
        let answer = answer_on(&mut cluster.client().await?, &read).await?;
        let objects = cells(&answer, 0)?;
        let object = objects.first().context("the literal")?;
        assert_eq!(object.annotation.as_deref(), Some(datatype_iri));
        let hash = Hash128::of(datatype_iri.as_bytes());
        assert_eq!(object.encoding.get(1..17), Some(hash.as_bytes().as_slice()));
        assert_eq!(
            object.encoding.get(..1),
            held.as_bytes().get(..1),
            "the literal's tag kept"
        );
        assert_eq!(
            object.encoding.get(17..),
            held.as_bytes().get(17..),
            "its lexical form kept"
        );
        Ok(())
    })
}

/// A resume key names its last row's literal object by its lexical form, read from the dictionary for one held by hash, and an
/// `xsd:string` by no datatype; a typed literal by its datatype's IRI (`PROTOCOL.md` §11.8).
#[test]
fn test_a_resume_key_names_a_literal_by_its_lexical_form() -> TestResult {
    run(async {
        let (cluster, tenant, predicate) = cluster().await?;
        let part = cluster.partition(1)?;
        let subject = id_in(first_bucket(tenant), 1)?;
        name(part, tenant, SUBJECT, subject);
        let long = "a string longer than fifteen bytes";
        let integer = "http://www.w3.org/2001/XMLSchema#integer";
        let literals = [
            Literal::new_simple_literal(long),
            typed("42", integer),
            Literal::new_simple_literal("after"),
        ];
        let mut list = Vec::new();
        for literal in &literals {
            list.push((literal_held(part, tenant, literal, None)?, Vec::new()));
        }
        part.put(forward_key(tenant, predicate, subject), list_of(&list));
        let pattern = Pattern::of(Some(SUBJECT), Some(PREDICATE), None);
        let expected: [(&str, Option<&str>); 2] = [(long, None), ("42", Some(integer))];
        for (limit, (lexical, datatype)) in (1..).zip(expected) {
            let read = pattern
                .read(tenant, true, ReadLevel::Latest)?
                .with_limit(limit);
            let answer = answer_on(&mut cluster.client().await?, &read).await?;
            let key = answer.resume();
            let view = ResumeKeyView::read(&key)?;
            let last = view
                .terms()
                .get(view.row().object())
                .context("the last row's object")?;
            assert_eq!(last, RequestTermView::Literal { lexical, datatype });
        }
        Ok(())
    })
}
