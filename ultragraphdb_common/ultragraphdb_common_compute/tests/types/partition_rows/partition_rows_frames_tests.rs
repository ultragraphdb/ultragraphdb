/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The rows an Entries frame gives (`PROTOCOL.md` §13.14): an object record held once is one row, one held under statements is a
//! row per statement with its reifier, and a statement's record is a row per property, each filtered by the pattern and what a
//! resumed read has passed.

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_protocol_wire::RequestTerm;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::played::played_cluster::{PlayedCluster, quiet_times, run};
use crate::played::played_data::{
    Term, forward, literal, records, statement_key, statement_record,
};
use crate::played::played_partition::PlayedPartition;
use crate::played::played_read::{Bound, read, snapshot_from, snapshot_of};

/// A bound literal object takes its record of the list alone.
#[test]
fn test_a_bound_object_takes_its_record_alone() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let partition = played.partition(1)?;
        let (subject, predicate) = (
            Term::resource("http://example.org/s", 1)?,
            Term::resource("http://example.org/p", 2)?,
        );
        subject.name_in(partition);
        predicate.name_in(partition);
        let mut objects = Vec::new();
        for lexical in ["a", "b", "c"] {
            objects.push((literal(lexical, None)?.as_bytes().to_vec(), Vec::new()));
        }
        objects.sort();
        partition.put(forward(&predicate, &subject), records(&objects));
        let bound = Bound {
            subject: Some(subject.iri.clone()),
            predicate: Some(predicate.iri.clone()),
            object: Some(RequestTerm::Literal {
                lexical: "b".to_string(),
                datatype: None,
            }),
        };
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
        assert_eq!(answer.texts(), vec![vec![None]]);
        Ok(())
    })
}

/// Holds in `partition` a list of two objects, the first under three statements: the bound pattern reading it, and its rows
/// `[object, reifier]` in the list's order, one per statement in ascending statement ID.
fn statements_in(partition: &PlayedPartition) -> TestResult<(Bound, Vec<Vec<Option<String>>>)> {
    let subject = Term::resource("http://example.org/s", 1)?;
    let predicate = Term::resource("http://example.org/p", 2)?;
    let reified = Term::resource("http://example.org/o1", 3)?;
    let plain = Term::resource("http://example.org/o2", 4)?;
    let mut reifiers = (1..=3)
        .map(|sequence| {
            Term::statement(
                &format!("http://example.org/.well-known/genid/r{sequence}"),
                &subject,
                sequence,
            )
        })
        .collect::<TestResult<Vec<_>>>()?;
    reifiers.sort_by_key(|reifier| reifier.id);
    [&subject, &predicate, &reified, &plain]
        .into_iter()
        .chain(&reifiers)
        .for_each(|term| term.name_in(partition));
    let statements: Vec<SystemId> = reifiers.iter().map(|reifier| reifier.id).collect();
    let mut held = vec![
        (reified.encoding(), statements),
        (plain.encoding(), Vec::new()),
    ];
    held.sort();
    partition.put(forward(&predicate, &subject), records(&held));
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    for (encoding, _) in &held {
        if *encoding == reified.encoding() {
            rows.extend(
                reifiers
                    .iter()
                    .map(|reifier| vec![Some(reified.iri.clone()), Some(reifier.iri.clone())]),
            );
        } else {
            rows.push(vec![Some(plain.iri.clone()), None]);
        }
    }
    let bound = Bound {
        subject: Some(subject.iri.clone()),
        predicate: Some(predicate.iri.clone()),
        object: None,
    };
    Ok((bound, rows))
}

/// An object held under statements is a row per statement, each with its reifier, in ascending statement ID; a read resumed at
/// one of them goes on with the next.
#[test]
fn test_an_object_held_under_statements_is_a_row_per_statement() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let (bound, rows) = statements_in(played.partition(1)?)?;
        let mut client = played.client().await?;
        let whole = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
        assert_eq!(whole.texts(), rows);
        let at = rows
            .iter()
            .position(|row| row.get(1).is_some_and(Option::is_some))
            .context("a reified row")?
            + 1;
        let limit = u32::try_from(at + 1)?;
        let first = read(&mut client, &snapshot_of(&bound)?.with_limit(limit))
            .await?
            .answered()?;
        let rest = read(&mut client, &snapshot_from(&bound, first.resume())?)
            .await?
            .answered()?;
        let past = rows
            .get(at + 1..)
            .context("the rows past the resumed one")?;
        assert_eq!(rest.texts(), past.to_vec());
        Ok(())
    })
}

/// A statement whose record holds a property under each of two predicates, the second twice: the reifier, its record and the
/// properties' rows `[predicate, value, reifier]` in the record's order, by predicate ID then value.
struct Reified {
    reifier: Term,
    first: Term,
    second: Term,
    values: Vec<Term>,
    rows: Vec<Vec<Option<String>>>,
}

/// Holds a statement's record with three properties in `partition`.
fn reified_in(partition: &PlayedPartition) -> TestResult<Reified> {
    let subject = Term::resource("http://example.org/s", 1)?;
    let (predicate, object) = (
        Term::resource("http://example.org/p", 2)?,
        Term::resource("http://example.org/o", 3)?,
    );
    let reifier = Term::statement("http://example.org/.well-known/genid/r", &subject, 1)?;
    let (first, second) = (
        Term::resource("http://example.org/q1", 4)?,
        Term::resource("http://example.org/q2", 5)?,
    );
    let values = vec![
        Term::resource("http://example.org/v1", 6)?,
        Term::resource("http://example.org/v2", 7)?,
        Term::resource("http://example.org/v3", 8)?,
    ];
    let mut properties = vec![
        (&first, &values[0]),
        (&second, &values[1]),
        (&second, &values[2]),
    ];
    properties.sort_by_key(|(property, value)| (property.id, value.encoding()));
    [&subject, &predicate, &object, &reifier, &first, &second]
        .into_iter()
        .chain(&values)
        .for_each(|term| term.name_in(partition));
    partition.put(
        statement_key(&reifier),
        statement_record(&subject, &predicate, &object, &properties),
    );
    let rows = properties
        .iter()
        .map(|(property, value)| vec![Some(property.iri.clone()), Some(value.iri.clone()), None])
        .collect();
    Ok(Reified {
        reifier,
        first,
        second,
        values,
        rows,
    })
}

/// A bound reifier alone reads its statement's record: a row per property, in the record's order, and a read resumed in it goes
/// on after the property it stopped at.
#[test]
fn test_a_bound_reifier_reads_its_statements_properties() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let reified = reified_in(played.partition(1)?)?;
        let bound = Bound {
            subject: Some(reified.reifier.iri.clone()),
            predicate: None,
            object: None,
        };
        let mut client = played.client().await?;
        let whole = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
        assert_eq!(whole.texts(), reified.rows);
        let first = read(&mut client, &snapshot_of(&bound)?.with_limit(1))
            .await?
            .answered()?;
        let rest = read(&mut client, &snapshot_from(&bound, first.resume())?)
            .await?
            .answered()?;
        assert_eq!(
            rest.texts(),
            reified
                .rows
                .get(1..)
                .context("the rows past the first")?
                .to_vec()
        );
        Ok(())
    })
}

/// A bound reifier with a bound predicate or object takes the record's properties that match it alone.
#[test]
fn test_a_bound_reifier_takes_the_properties_matching_the_pattern() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let reified = reified_in(played.partition(1)?)?;
        let mut client = played.client().await?;
        for property in [&reified.first, &reified.second] {
            let bound = Bound {
                subject: Some(reified.reifier.iri.clone()),
                predicate: Some(property.iri.clone()),
                object: None,
            };
            let answer = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
            let expected: Vec<Vec<Option<String>>> = reified
                .rows
                .iter()
                .filter(|row| row.first() == Some(&Some(property.iri.clone())))
                .map(|row| row.iter().skip(1).cloned().collect())
                .collect();
            assert_eq!(answer.texts(), expected);
        }
        let value = reified.values.get(1).context("a value")?;
        let bound = Bound {
            subject: Some(reified.reifier.iri.clone()),
            predicate: None,
            object: Some(RequestTerm::Iri(value.iri.clone())),
        };
        let answer = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
        assert_eq!(
            answer.texts(),
            vec![vec![Some(reified.second.iri.clone()), None]]
        );
        Ok(())
    })
}
