/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A partition's rows by their terms (`PROTOCOL.md` §11.12, §13.12): each unbound position named from the partition's dictionary,
//! read a page ahead of the merge in one Scan, and the resume key of the row a limit stopped at.

use ultragraphdb_common_compute::read_times_of;
use ultragraphdb_core_keys::Graph;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{RequestTermView, ResumeKeyView};
use ultragraphdb_store_quads::id_key;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::played_answers::{
    GraphTerm, Pattern, answer_on, next_of, pattern_hash, snapshot,
};
use crate::utils::played_cluster::{
    Played, ROWS_PREDICATE, TENANT, cells_of, hold_rows, id, run, short_rows,
};

const P: &str = ROWS_PREDICATE;

/// An IRI of about `bytes` bytes, distinct by `n`.
fn long_iri(n: usize, bytes: usize) -> String {
    format!("http://ex.org/{n}/{}", "a".repeat(bytes))
}

/// An unbound graph is walked, each row naming its graph, subject and object by the partition's dictionary; the default graph
/// names none.
#[test]
fn test_every_unbound_position_is_named_by_the_dictionary() -> TestResult {
    run(async {
        let played = Played::start(1, read_times_of(None, None)).await?;
        let partition = played.partition(1)?;
        let rows = short_rows(2);
        let ids = hold_rows(partition, rows.get(..1).context("a row")?)?;
        let graph = "http://ex.org/graph";
        let (g, s, o) = (id(30)?, id(31)?, id(32)?);
        partition.term(graph, g);
        let (subject, object) = rows.get(1).context("a row")?;
        partition.term(subject, s);
        partition.term(object, o);
        partition.quads(Graph::Named(g), s, id(1)?, &[EncodedTerm::id(o)]);
        let mut client = played.client().await?;
        let pattern = Pattern::of(None, Some(P), None).in_graph(GraphTerm::Unbound);
        let mut answered = answer_on(&mut client, &snapshot(&pattern)?)
            .await?
            .whole()?;
        answered.sort();
        let (first_subject, first_object) = rows.first().context("a row")?;
        let mut expected = vec![
            vec![
                None,
                Some(first_subject.clone()),
                Some(first_object.clone()),
                None,
            ],
            vec![
                Some(graph.to_string()),
                Some(subject.clone()),
                Some(object.clone()),
                None,
            ],
        ];
        expected.sort();
        assert_eq!(answered, expected);
        assert_eq!(ids.len(), 1);
        Ok(())
    })
}

/// The names of a page's rows are read in one Scan of one-key ranges, each row's subject and object once.
#[test]
fn test_one_dictionary_scan_names_a_pages_rows() -> TestResult {
    run(async {
        let played = Played::start(1, read_times_of(None, None)).await?;
        let partition = played.partition(1)?;
        let rows = short_rows(5);
        let ids = hold_rows(partition, &rows)?;
        let mut client = played.client().await?;
        let read = snapshot(&Pattern::of(None, Some(P), None))?;
        assert_eq!(
            answer_on(&mut client, &read).await?.whole()?,
            cells_of(&rows)
        );
        let named: Vec<Vec<u8>> = ids
            .iter()
            .flat_map(|(s, o)| [id_key(TENANT, *s), id_key(TENANT, *o)])
            .collect();
        let names: Vec<Vec<Vec<u8>>> = partition
            .scans()
            .into_iter()
            .map(|scan| {
                scan.ranges
                    .into_iter()
                    .map(|range| range.first)
                    .collect::<Vec<_>>()
            })
            .filter(|firsts| firsts.iter().all(|first| named.contains(first)))
            .collect();
        assert_eq!(names, vec![named]);
        Ok(())
    })
}

/// A page whose names come to more than a Result frame's worth is named in parts, the merge going on between them, and every row
/// is answered by its names.
#[test]
fn test_a_page_whose_names_pass_a_frame_is_named_in_parts() -> TestResult {
    run(async {
        let played = Played::start(1, read_times_of(None, None)).await?;
        let partition = played.partition(1)?;
        let rows: Vec<(String, String)> = (0..4)
            .map(|i| (format!("http://ex.org/s{i}"), long_iri(i, 400_000)))
            .collect();
        let ids = hold_rows(partition, &rows)?;
        let mut client = played.client().await?;
        let read = snapshot(&Pattern::of(None, Some(P), None))?;
        assert_eq!(
            answer_on(&mut client, &read).await?.whole()?,
            cells_of(&rows)
        );
        let last_object = id_key(TENANT, ids.last().context("a row")?.1);
        let asked_twice = partition
            .scans()
            .iter()
            .filter(|scan| scan.ranges.iter().any(|range| range.first == last_object))
            .count();
        assert_eq!(
            asked_twice, 2,
            "the last row's names are asked again once the first part is used"
        );
        Ok(())
    })
}

/// A row whose stored object does not read refuses the read with code 17 naming its list, once the rows before it have gone as far
/// as the merge has sent them.
#[test]
fn test_a_row_that_does_not_read_refuses_the_read_after_the_rows_before_it() -> TestResult {
    run(async {
        let played = Played::start(1, read_times_of(None, None)).await?;
        let partition = played.partition(1)?;
        let rows: Vec<(String, String)> = (0..2)
            .map(|i| (format!("http://ex.org/s{i}"), long_iri(i, 300_000)))
            .collect();
        let ids = hold_rows(partition, &rows)?;
        let corrupt = id(40)?;
        partition.term("http://ex.org/s9", corrupt);
        let key = ultragraphdb_core_keys::QuadKey::Forward {
            tenant: TENANT,
            graph: Graph::Default,
            predicate: id(1)?,
            subject: corrupt,
        };
        let mut records = ultragraphdb_protocol_wire::ObjectRecordsBuilder::new();
        records.push(&[0xFF, 0xFF], &[]);
        partition.put(key.to_bytes(), records.value().to_vec());
        assert!(
            ids.iter().all(|(s, _)| *s < corrupt),
            "the corrupt row is the last"
        );
        let mut client = played.client().await?;
        client
            .send_only(&snapshot(&Pattern::of(None, Some(P), None))?)
            .await?;
        let first = next_of(&mut client).await?;
        let batch = first.ok().context("the first row's batch")?;
        assert_eq!(batch.rows, cells_of(rows.get(..1).context("a row")?));
        Ok(())
    })
}

/// A read stopped by its limit ends with the resume key of the row it returned last: the read's pattern and version, and that
/// row's terms by their text.
#[test]
fn test_a_limit_ends_with_the_resume_key_of_the_last_row() -> TestResult {
    run(async {
        let played = Played::start(1, read_times_of(None, None)).await?;
        let rows = short_rows(2);
        let _ids = hold_rows(played.partition(1)?, &rows)?;
        let mut client = played.client().await?;
        let read = snapshot(&Pattern::of(None, Some(P), None))?.with_limit(1);
        let answered = answer_on(&mut client, &read).await?;
        assert_eq!(answered.whole()?, cells_of(rows.get(..1).context("a row")?));
        let resume = answered.batches.last().context("a batch")?.resume.clone();
        let key = ResumeKeyView::read(&resume)?;
        assert_eq!(key.pattern(), pattern_hash(&read)?);
        assert_eq!(key.scope(), read.read_version());
        let row = key.row();
        let text = |index: u32| match key.terms().get(index) {
            Some(RequestTermView::Iri(iri)) => Some(iri.to_string()),
            _ => None,
        };
        let (subject, object) = rows.first().context("a row")?;
        assert_eq!(row.graph(), None);
        assert_eq!(row.reifier(), None);
        assert_eq!(text(row.subject()).as_ref(), Some(subject));
        assert_eq!(text(row.predicate()).as_deref(), Some(P));
        assert_eq!(text(row.object()).as_ref(), Some(object));
        Ok(())
    })
}
