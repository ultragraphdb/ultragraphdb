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

/// `pattern` read at latest level in `tenant`, from `resume`.
fn resumed(
    pattern: &Pattern,
    tenant: u64,
    resume: Vec<u8>,
) -> TestResult<ultragraphdb_protocol_wire::ComputeRead> {
    let (terms, slots) = pattern.table()?;
    Ok(ultragraphdb_protocol_wire::ComputeRead::new(
        tenant,
        true,
        ReadLevel::Latest,
        terms,
        slots,
        resume,
    )
    .with_principal(1)
    .with_deadline_ms(crate::played_reads::read_answer::DEADLINE_MS)
    .with_client_limits(ultragraphdb_protocol_wire::MAX_FRAME_SIZE_U32, 4))
}

/// In the store's order: a string, the language strings `tagged` and one of each direction, a known datatype's literal not in its
/// canonical form, and two of `datatype`, held by ID, so every kind but the last stands as a resume key's row.
fn every_kind(tagged: &Literal, datatype: &str) -> [Literal; 7] {
    let directed = |direction| {
        Literal::new_directional_language_tagged_literal_unchecked(
            "a directed text",
            "en",
            direction,
        )
    };
    [
        Literal::new_simple_literal("after"),
        tagged.clone(),
        directed(oxrdf::BaseDirection::Ltr),
        directed(oxrdf::BaseDirection::Rtl),
        typed("007", "http://www.w3.org/2001/XMLSchema#integer"),
        typed("v", datatype),
        typed("w", datatype),
    ]
}

/// A language-tagged literal held by hash comes back by its texts, and a resume key naming a language string of any direction, a
/// known datatype's literal or one whose datatype the store names by ID continues the read after its row (`PROTOCOL.md` §10.3,
/// §11.8).
#[test]
fn test_a_read_resumed_at_a_tagged_or_typed_literal_goes_on_after_it() -> TestResult {
    run(async {
        let (cluster, tenant, predicate) = cluster().await?;
        let part = cluster.partition(1)?;
        let bucket = first_bucket(tenant);
        let subject = id_in(bucket, 1)?;
        let datatype = id_in(bucket, 40)?;
        let datatype_iri = "http://ex.org/datatype";
        name(part, tenant, SUBJECT, subject);
        name(part, tenant, datatype_iri, datatype);
        let tagged = Literal::new_language_tagged_literal_unchecked(
            "a language-tagged text longer than fifteen bytes",
            "en-x-private-tagging",
        );
        let literals = every_kind(&tagged, datatype_iri);
        let mut list = Vec::new();
        for literal in &literals {
            let named = (literal.datatype().as_str() == datatype_iri).then_some(datatype);
            list.push((literal_held(part, tenant, literal, named)?, Vec::new()));
        }
        // The list in the store's order of encodings, as a list holds its objects.
        list.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        part.put(forward_key(tenant, predicate, subject), list_of(&list));
        let pattern = Pattern::of(Some(SUBJECT), Some(PREDICATE), None);
        let whole = answer_on(
            &mut cluster.client().await?,
            &pattern.read(tenant, true, ReadLevel::Latest)?,
        )
        .await?;
        let every = cells(&whole, 0)?;
        assert_eq!(every.len(), literals.len());
        let tagged_cell = every
            .iter()
            .find(|cell| cell.text == tagged.value())
            .context("the tagged literal's cell, by its text")?;
        assert_eq!(tagged_cell.annotation.as_deref(), tagged.language());
        for limit in 1..literals.len() {
            let first = answer_on(
                &mut cluster.client().await?,
                &pattern
                    .read(tenant, true, ReadLevel::Latest)?
                    .with_limit(u32::try_from(limit)?),
            )
            .await?;
            let rest = answer_on(
                &mut cluster.client().await?,
                &resumed(&pattern, tenant, first.resume())?,
            )
            .await?;
            rest.ended()?;
            assert_eq!(
                cells(&rest, 0)?,
                every.get(limit..).context("the rows after the key's")?,
                "resumed after row {limit}"
            );
        }
        Ok(())
    })
}
