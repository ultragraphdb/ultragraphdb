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
