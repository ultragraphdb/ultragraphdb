/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! How a partition reads a pattern by its bound positions (`PROTOCOL.md` §11.12, quad-reads "Access paths follow the bound
//! positions"), and where a resumed read goes on (`PROTOCOL.md` §11.8, §13.13).

use ultragraphdb_core_terms::{FLOAT32_VECTOR, format_vector};
use ultragraphdb_protocol_wire::RequestTerm;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::played::played_cluster::{PlayedCluster, quiet_times, run};
use crate::played::played_data::{
    OneList, Term, TwoLists, forward, literal, plain, records, reverse, subject_predicates,
};
use crate::played::played_read::{Bound, read, snapshot_from, snapshot_of};

/// A bound IRI object reads the subject's list for its one record.
#[test]
fn test_a_bound_iri_object_reads_its_one_record() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let list = OneList::new("subject", "predicate", 3)?;
        list.hold_in(played.partition(1)?);
        let object = list.objects.get(1).context("an object")?;
        let bound = Bound {
            subject: Some(list.subject.iri.clone()),
            predicate: Some(list.predicate.iri.clone()),
            object: Some(RequestTerm::Iri(object.iri.clone())),
        };
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
        assert_eq!(answer.texts(), vec![vec![None]]);
        Ok(())
    })
}

/// A bound literal object with no subject reads the predicate's reverse list of that object.
#[test]
fn test_a_bound_literal_object_alone_reads_its_reverse_list() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let partition = played.partition(1)?;
        let (subject, predicate) = (
            Term::resource("http://example.org/s", 1)?,
            Term::resource("http://example.org/p", 2)?,
        );
        subject.name_in(partition);
        predicate.name_in(partition);
        let object = literal("a value", None)?;
        partition.put(
            forward(&predicate, &subject),
            records(&[(object.as_bytes().to_vec(), Vec::new())]),
        );
        let key = reverse(&predicate, &object);
        partition.put(key.clone(), plain(&[&subject]));
        let bound = Bound {
            subject: None,
            predicate: Some(predicate.iri.clone()),
            object: Some(RequestTerm::Literal {
                lexical: "a value".to_string(),
                datatype: None,
            }),
        };
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
        assert_eq!(answer.column(0), vec![Some(subject.iri.clone())]);
        let scanned = partition
            .scans()
            .iter()
            .any(|scan| scan.ranges.iter().any(|range| range.first == key));
        assert!(scanned, "{:?}", partition.scans());
        Ok(())
    })
}

/// A bound vector object, which no reverse key names, reads the predicate's forward lists for it.
#[test]
fn test_a_bound_vector_object_reads_the_forward_lists() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let partition = played.partition(1)?;
        let predicate = Term::resource("http://example.org/p", 1)?;
        let (held, other) = (
            format_vector([1.0, 2.5].into_iter()),
            format_vector([3.0].into_iter()),
        );
        let mut subjects = Vec::new();
        for (sequence, lexical) in [(2, &held), (3, &other)] {
            let subject = Term::resource(&format!("http://example.org/s{sequence}"), sequence)?;
            subject.name_in(partition);
            let value = literal(lexical, Some(FLOAT32_VECTOR))?;
            partition.put(
                forward(&predicate, &subject),
                records(&[(value.as_bytes().to_vec(), Vec::new())]),
            );
            subjects.push(subject);
        }
        predicate.name_in(partition);
        let bound = Bound {
            subject: None,
            predicate: Some(predicate.iri.clone()),
            object: Some(RequestTerm::Literal {
                lexical: held,
                datatype: Some(FLOAT32_VECTOR.to_string()),
            }),
        };
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
        let first = subjects.first().context("a subject")?;
        assert_eq!(answer.column(0), vec![Some(first.iri.clone())]);
        Ok(())
    })
}

/// A reverse read whose object's home names no referrer reads no list there: the home is read for its referrer set alone.
#[test]
fn test_a_reverse_read_of_no_referrer_reads_no_list_at_the_home() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let home = played.partition(1)?;
        let (subject, predicate, object) = (
            Term::resource("http://example.org/s", 1)?,
            Term::resource("http://example.org/p", 2)?,
            Term::resource("http://example.org/o", 3)?,
        );
        [&subject, &predicate, &object]
            .into_iter()
            .for_each(|term| term.name_in(home));
        let key = reverse(&predicate, &object.id_term());
        home.put(key.clone(), plain(&[&subject]));
        let bound = Bound {
            subject: None,
            predicate: Some(predicate.iri.clone()),
            object: Some(RequestTerm::Iri(object.iri.clone())),
        };
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
        assert_eq!(answer.texts(), Vec::<Vec<Option<String>>>::new());
        let listed = home
            .scans()
            .iter()
            .any(|scan| scan.ranges.iter().any(|range| range.first == key));
        assert!(!listed, "{:?}", home.scans());
        Ok(())
    })
}

/// A read resumed in a list goes on after the row its key names: the list asked from that object on, the object itself dropped.
#[test]
fn test_a_read_resumed_in_a_list_goes_on_after_its_last_row() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let partition = played.partition(1)?;
        let list = OneList::new("subject", "predicate", 4)?;
        list.hold_in(partition);
        let bound = Bound {
            subject: Some(list.subject.iri.clone()),
            predicate: Some(list.predicate.iri.clone()),
            object: None,
        };
        let mut client = played.client().await?;
        let first = read(&mut client, &snapshot_of(&bound)?.with_limit(2))
            .await?
            .answered()?;
        let objects = list.object_iris();
        assert_eq!(
            first.column(0),
            objects.get(..2).context("two objects")?.to_vec()
        );
        let asked = partition.scans().len();
        let rest = read(&mut client, &snapshot_from(&bound, first.resume())?)
            .await?
            .answered()?;
        assert_eq!(
            rest.column(0),
            objects.get(2..).context("two objects")?.to_vec()
        );
        let last = list.objects.get(1).context("the last object returned")?;
        let key = forward(&list.predicate, &list.subject);
        let resumed = partition
            .scans()
            .into_iter()
            .skip(asked)
            .flat_map(|scan| scan.ranges)
            .find(|range| range.first == key);
        assert_eq!(resumed.map(|range| range.after), Some(last.encoding()));
        Ok(())
    })
}

/// A read resumed in a subject's predicates' list asks that list from the predicate it stopped in, and that predicate's list from
/// the object it stopped at.
#[test]
fn test_a_read_resumed_in_a_predicates_list_asks_from_where_it_stopped() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let partition = played.partition(1)?;
        let lists = TwoLists::new("subject")?;
        lists.hold_in(partition);
        let bound = Bound {
            subject: Some(lists.subject.iri.clone()),
            predicate: None,
            object: None,
        };
        let mut client = played.client().await?;
        let rows = lists.rows();
        let first = read(&mut client, &snapshot_of(&bound)?.with_limit(3))
            .await?
            .answered()?;
        assert_eq!(first.texts(), rows.get(..3).context("three rows")?.to_vec());
        let asked = partition.scans().len();
        let rest = read(&mut client, &snapshot_from(&bound, first.resume())?)
            .await?
            .answered()?;
        assert_eq!(rest.texts(), rows.get(3..).context("a row")?.to_vec());
        let second = lists.lists.get(1).context("the second list")?;
        let stopped = second.objects.first().context("its first object")?;
        let afters: Vec<(Vec<u8>, Vec<u8>)> = partition
            .scans()
            .into_iter()
            .skip(asked)
            .flat_map(|scan| scan.ranges)
            .map(|range| (range.first, range.after))
            .collect();
        let listed = (
            subject_predicates(&lists.subject),
            second.predicate.encoding(),
        );
        assert!(afters.contains(&listed), "{afters:?}");
        let resumed = (
            forward(&second.predicate, &second.subject),
            stopped.encoding(),
        );
        assert!(afters.contains(&resumed), "{afters:?}");
        Ok(())
    })
}

/// A bound term the partition's dictionary does not name means it holds no statement of it: it answers no row, whether
/// the predicate or the object is the term.
#[test]
fn test_a_bound_term_the_partition_does_not_name_answers_no_row() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let list = OneList::new("subject", "predicate", 1)?;
        list.hold_in(played.partition(1)?);
        let unnamed = Term::resource("http://example.org/unnamed", 9)?;
        for bound in [
            Bound {
                subject: Some(list.subject.iri.clone()),
                predicate: Some(unnamed.iri.clone()),
                object: None,
            },
            Bound {
                subject: Some(list.subject.iri.clone()),
                predicate: Some(list.predicate.iri.clone()),
                object: Some(RequestTerm::Iri(unnamed.iri.clone())),
            },
        ] {
            let mut client = played.client().await?;
            let answer = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
            assert!(answer.texts().is_empty(), "{bound:?}");
        }
        Ok(())
    })
}

/// Has `part` hold, in `tenant`'s named graph `g`, `<s> <p>` listing `objects`, every IRI named, the IDs in the run's first bucket.
fn hold_in_graph(
    part: &crate::played_reads::played_partition::PlayedPartition,
    tenant: u64,
    (s, p, g): (&str, &str, &str),
    objects: &[&str],
) -> TestResult {
    use ultragraphdb_core_keys::{Graph, QuadKey};

    use crate::played_reads::played_data::{first_bucket, id_in, ids_list, name};

    let bucket = first_bucket(tenant);
    let (subject, predicate, graph) = (id_in(bucket, 1)?, id_in(bucket, 2)?, id_in(bucket, 3)?);
    name(part, tenant, s, subject);
    name(part, tenant, p, predicate);
    name(part, tenant, g, graph);
    let mut ids = Vec::new();
    for (sequence, object) in (10..).zip(objects) {
        let id = id_in(bucket, sequence)?;
        name(part, tenant, object, id);
        ids.push(id);
    }
    let forward = QuadKey::Forward {
        tenant,
        graph: Graph::Named(graph),
        predicate,
        subject,
    };
    part.put(forward.to_bytes(), ids_list(&ids));
    Ok(())
}

/// A named graph, bound or walked: a bound graph the partition names reads its lists there, one it does not name answers no row,
/// and a read stopped by its limit in a named graph resumes in that graph, bound or walked (`PROTOCOL.md` §11.8, §11.12).
#[test]
fn test_a_read_in_a_named_graph_resumes_in_that_graph() -> TestResult {
    use ultragraphdb_common_compute::read_times_of;
    use ultragraphdb_protocol_wire::{ComputeRead, MAX_FRAME_SIZE_U32, ReadLevel};

    use crate::played_reads::played_cluster::PlayedCluster as ReadsCluster;
    use crate::played_reads::played_data::tenant_over;
    use crate::played_reads::read_answer::{DEADLINE_MS, Pattern, answer_on};

    const S: &str = "http://ex.org/s";
    const P: &str = "http://ex.org/p";
    const G: &str = "http://ex.org/g";
    const OBJECTS: [&str; 3] = ["http://ex.org/o1", "http://ex.org/o2", "http://ex.org/o3"];
    crate::played_reads::played_cluster::run(async {
        let cluster = ReadsCluster::start(1, read_times_of(None, None)).await?;
        let tenant = tenant_over(1, 1)?;
        let part = cluster.partition(1)?;
        hold_in_graph(part, tenant, (S, P, G), &OBJECTS)?;
        let read = |pattern: &Pattern, resume: Vec<u8>| -> TestResult<ComputeRead> {
            let (terms, slots) = pattern.table()?;
            Ok(
                ComputeRead::new(tenant, true, ReadLevel::Latest, terms, slots, resume)
                    .with_principal(1)
                    .with_deadline_ms(DEADLINE_MS)
                    .with_client_limits(MAX_FRAME_SIZE_U32, 4),
            )
        };
        let in_graph = |slot: Option<Option<String>>| {
            let mut pattern = Pattern::of(Some(S), Some(P), None);
            pattern.graph = slot;
            pattern
        };
        let absent = in_graph(Some(Some("http://ex.org/absent".to_string())));
        let none = answer_on(&mut cluster.client().await?, &read(&absent, Vec::new())?).await?;
        none.ended()?;
        assert!(
            none.rows().is_empty(),
            "a graph the partition does not name"
        );
        // Bound, the object is the row's first cell; walked, the graph is and the object second.
        for (pattern, column) in [
            (in_graph(Some(Some(G.to_string()))), 0),
            (in_graph(None), 1),
        ] {
            let first = answer_on(
                &mut cluster.client().await?,
                &read(&pattern, Vec::new())?.with_limit(1),
            )
            .await?;
            assert_eq!(first.column(column), OBJECTS[..1]);
            let rest = answer_on(
                &mut cluster.client().await?,
                &read(&pattern, first.resume())?,
            )
            .await?;
            rest.ended()?;
            assert_eq!(rest.column(column), OBJECTS[1..]);
            if column == 1 {
                assert_eq!(rest.column(0), [G; 2], "each row names its graph");
            }
        }
        Ok(())
    })
}
