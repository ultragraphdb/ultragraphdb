/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read names each row's terms by the texts its partition's dictionary holds (`PROTOCOL.md` §13.12), reading each text once while
//! it holds it.

use std::collections::HashSet;

use ultragraphdb_common_compute::read_times_of;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::ReadLevel;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::played_reads::played_cluster::{PlayedCluster, run};
use crate::played_reads::played_data::{
    forward_key, id_of, ids_list, list_of, literal_held, name, name_key, subject_predicates_key,
    tenant_over,
};
use crate::played_reads::played_partition::is_dictionary;
use crate::played_reads::read_answer::{Pattern, answer_on};

const SUBJECT: &str = "http://ex.org/s";
const PREDICATE: &str = "http://ex.org/p";
const OTHER_PREDICATE: &str = "http://ex.org/q";

/// The objects of `<s> <p> ?o` come back by their IRIs, in their list's order, each a plain IRI.
#[test]
fn test_a_read_names_each_object_by_its_dictionary_text() -> TestResult {
    run(async {
        let cluster = PlayedCluster::start(1, read_times_of(None, None)).await?;
        let tenant = tenant_over(1, 1)?;
        let part = cluster.partition(1)?;
        let (subject, predicate) = (id_of(tenant, 1)?, id_of(tenant, 2)?);
        name(part, tenant, SUBJECT, subject);
        name(part, tenant, PREDICATE, predicate);
        let objects = ["http://ex.org/o1", "http://ex.org/o2"];
        let mut ids = Vec::new();
        for (at, iri) in objects.iter().enumerate() {
            let id = id_of(tenant, 10 + at as u64)?;
            name(part, tenant, iri, id);
            ids.push(id);
        }
        part.put(forward_key(tenant, predicate, subject), ids_list(&ids));
        let read = Pattern::of(Some(SUBJECT), Some(PREDICATE), None).read(
            tenant,
            true,
            ReadLevel::Latest,
        )?;
        let answer = answer_on(&mut cluster.client().await?, &read).await?;
        let last = answer.whole()?;
        assert_eq!(last.columns, 2, "the object and the reifier");
        assert_eq!(answer.column(0), objects);
        let blank: Vec<bool> = answer
            .rows()
            .iter()
            .map(|row| {
                row.first()
                    .and_then(Clone::clone)
                    .is_some_and(|cell| cell.blank)
            })
            .collect();
        assert_eq!(blank, [false, false], "an IRI is no blank node");
        Ok(())
    })
}

/// `<s> ?p ?o` names each row's predicate as well as its object.
#[test]
fn test_a_read_names_each_predicate_by_its_dictionary_text() -> TestResult {
    run(async {
        let cluster = PlayedCluster::start(1, read_times_of(None, None)).await?;
        let tenant = tenant_over(1, 1)?;
        let part = cluster.partition(1)?;
        let held = two_predicates(part, tenant)?;
        let read = Pattern::of(Some(SUBJECT), None, None).read(tenant, true, ReadLevel::Latest)?;
        let answer = answer_on(&mut cluster.client().await?, &read).await?;
        answer.ended()?;
        assert_eq!(answer.column(0), [PREDICATE, OTHER_PREDICATE]);
        assert_eq!(answer.column(1), ["http://ex.org/o1", "http://ex.org/o1"]);
        let asked = names_asked(part, tenant, &held)?;
        assert_eq!(
            asked, 3,
            "each name read once: the two predicates and the one object"
        );
        Ok(())
    })
}

/// The IDs `<s>` holds under two predicates, each listing one object, the same: the subject, the predicates and the object.
fn two_predicates(
    part: &crate::played_reads::played_partition::PlayedPartition,
    tenant: u64,
) -> TestResult<Vec<ultragraphdb_core_ids::SystemId>> {
    let subject = id_of(tenant, 1)?;
    let predicates = [id_of(tenant, 2)?, id_of(tenant, 3)?];
    let object = id_of(tenant, 10)?;
    name(part, tenant, SUBJECT, subject);
    name(part, tenant, PREDICATE, predicates[0]);
    name(part, tenant, OTHER_PREDICATE, predicates[1]);
    name(part, tenant, "http://ex.org/o1", object);
    part.put(
        subject_predicates_key(tenant, subject),
        ids_list(&predicates),
    );
    for predicate in predicates {
        part.put(forward_key(tenant, predicate, subject), ids_list(&[object]));
    }
    Ok(vec![subject, predicates[0], predicates[1], object])
}

/// How many dictionary ranges naming `ids` by their keys `part` was asked for, failing should one be asked twice in a Scan.
fn names_asked(
    part: &crate::played_reads::played_partition::PlayedPartition,
    tenant: u64,
    ids: &[ultragraphdb_core_ids::SystemId],
) -> TestResult<usize> {
    let keys: HashSet<Vec<u8>> = ids.iter().map(|id| name_key(tenant, *id)).collect();
    let mut asked = 0;
    for scan in part.read_scans() {
        let named: Vec<&Vec<u8>> = scan
            .ranges
            .iter()
            .map(|range| &range.first)
            .filter(|first| is_dictionary(first) && keys.contains(*first))
            .collect();
        let unique: HashSet<&Vec<u8>> = named.iter().copied().collect();
        assert_eq!(unique.len(), named.len(), "a Scan asks a name once");
        asked += named.len();
    }
    Ok(asked)
}

/// A string literal over 15 bytes is held by hash, and comes back by the text the dictionary holds for it.
#[test]
fn test_a_long_literal_is_named_by_its_hashed_text() -> TestResult {
    run(async {
        let cluster = PlayedCluster::start(1, read_times_of(None, None)).await?;
        let tenant = tenant_over(1, 1)?;
        let part = cluster.partition(1)?;
        let (subject, predicate) = (id_of(tenant, 1)?, id_of(tenant, 2)?);
        name(part, tenant, SUBJECT, subject);
        name(part, tenant, PREDICATE, predicate);
        let text = "a string longer than fifteen bytes";
        let literal = oxrdf::Literal::new_simple_literal(text);
        let encoded = literal_held(part, tenant, &literal, None)?;
        part.put(
            forward_key(tenant, predicate, subject),
            list_of(&[(encoded.clone(), Vec::new())]),
        );
        let read = Pattern::of(Some(SUBJECT), Some(PREDICATE), None).read(
            tenant,
            true,
            ReadLevel::Latest,
        )?;
        let answer = answer_on(&mut cluster.client().await?, &read).await?;
        answer.ended()?;
        let rows = answer.rows();
        let cell = rows
            .first()
            .and_then(|row| row.first())
            .and_then(Clone::clone)
            .context("the literal's cell")?;
        assert_eq!(cell.text, text);
        assert_eq!(EncodedTerm::from_bytes(&cell.encoding)?, encoded);
        Ok(())
    })
}

/// The texts a read holds per partition before it lets them go, as `names.rs` bounds them: `TEXTS_MAX`, 2^14.
const TEXTS_MAX: usize = 1 << 14;

/// The text bytes a read holds per partition before it lets them go, keys and texts counted, as `names.rs` bounds them:
/// `TEXT_BYTES_MAX`, 4 MiB.
const TEXT_BYTES_MAX: usize = 4 << 20;

/// `<s>` holds `<p>` listing `objects`, named by `texts`, and `<q>` listing the first of them again: the first object's ID.
fn first_listed_twice(
    part: &crate::played_reads::played_partition::PlayedPartition,
    tenant: u64,
    texts: &[String],
) -> TestResult<ultragraphdb_core_ids::SystemId> {
    let subject = id_of(tenant, 1)?;
    let predicates = [id_of(tenant, 2)?, id_of(tenant, 3)?];
    name(part, tenant, SUBJECT, subject);
    name(part, tenant, PREDICATE, predicates[0]);
    name(part, tenant, OTHER_PREDICATE, predicates[1]);
    let mut objects = Vec::with_capacity(texts.len());
    for (at, text) in (100..).zip(texts) {
        let id = id_of(tenant, at)?;
        name(part, tenant, text, id);
        objects.push(id);
    }
    let first = objects.first().copied().context("an object")?;
    part.put(
        subject_predicates_key(tenant, subject),
        ids_list(&predicates),
    );
    part.put(
        forward_key(tenant, predicates[0], subject),
        ids_list(&objects),
    );
    part.put(
        forward_key(tenant, predicates[1], subject),
        ids_list(&[first]),
    );
    Ok(first)
}

/// How many times `<s> ?p ?o` over `texts` asks the first object's name: once while the read holds it, twice once the texts held
/// before the second list passed a bound.
async fn first_asked(texts: &[String]) -> TestResult<usize> {
    let cluster = PlayedCluster::start(1, read_times_of(None, None)).await?;
    let tenant = tenant_over(1, 1)?;
    let part = cluster.partition(1)?;
    let first = first_listed_twice(part, tenant, texts)?;
    let read = Pattern::of(Some(SUBJECT), None, None).read(tenant, true, ReadLevel::Latest)?;
    let answer = answer_on(&mut cluster.client().await?, &read).await?;
    answer.ended()?;
    assert_eq!(answer.rows().len(), texts.len() + 1);
    let key = name_key(tenant, first);
    Ok(part
        .read_scans()
        .iter()
        .flat_map(|scan| scan.ranges.iter())
        .filter(|range| range.first == key)
        .count())
}

/// Texts held up to the bound are kept: the first list's predicate and its objects, exactly `TEXTS_MAX` texts, leave the first
/// object named for the second list. One past the bound lets them go, and the first object is read again.
#[test]
fn test_a_read_lets_its_texts_go_past_their_count_bound() -> TestResult {
    run(async {
        let at_bound: Vec<String> = (0..TEXTS_MAX - 1)
            .map(|at| format!("http://ex.org/o{at}"))
            .collect();
        assert_eq!(
            first_asked(&at_bound).await?,
            1,
            "{TEXTS_MAX} texts are kept"
        );
        let past: Vec<String> = (0..TEXTS_MAX)
            .map(|at| format!("http://ex.org/o{at}"))
            .collect();
        assert_eq!(
            first_asked(&past).await?,
            2,
            "one past {TEXTS_MAX} lets them go"
        );
        Ok(())
    })
}

/// Five object texts whose keys and texts, with the predicate's, come to `total` bytes.
fn texts_of(tenant: u64, total: usize) -> TestResult<Vec<String>> {
    let key = name_key(tenant, id_of(tenant, 2)?).len();
    let held = key + PREDICATE.len();
    let each = (total - held) / 5 - key;
    let mut texts: Vec<String> = (0..5)
        .map(|at| {
            let stem = format!("http://ex.org/{at}/");
            format!("{stem}{}", "a".repeat(each - stem.len()))
        })
        .collect();
    let short = total - held - texts.iter().map(|text| key + text.len()).sum::<usize>();
    texts
        .last_mut()
        .context("a text")?
        .push_str(&"b".repeat(short));
    Ok(texts)
}

/// Texts of `TEXT_BYTES_MAX` bytes held, keys counted, are kept; one byte more lets them go.
#[test]
fn test_a_read_lets_its_texts_go_past_their_byte_bound() -> TestResult {
    run(async {
        let tenant = tenant_over(1, 1)?;
        let at_bound = texts_of(tenant, TEXT_BYTES_MAX)?;
        assert_eq!(
            first_asked(&at_bound).await?,
            1,
            "{TEXT_BYTES_MAX} bytes are kept"
        );
        let past = texts_of(tenant, TEXT_BYTES_MAX + 1)?;
        assert_eq!(
            first_asked(&past).await?,
            2,
            "one byte past {TEXT_BYTES_MAX} lets them go"
        );
        Ok(())
    })
}
