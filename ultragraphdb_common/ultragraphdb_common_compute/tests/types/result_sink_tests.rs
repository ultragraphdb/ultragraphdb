/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read's Result batches (`PROTOCOL.md` §13.12): each within the client's `max_frame`, a row or resume key too large for any
//! refused with code 6, the last flagged and carrying the resume key, and the read ended once its last batch has gone.

use std::time::Duration;

use oxrdf::{Literal, NamedNode};
use ultragraphdb_common_compute::read_times_of;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::Graph;
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_protocol_wire::{
    Credit, ErrorCode, MAX_FRAME_FLOOR, ResultBatch, ResultTerm, ResumeKey, ResumeRow, TermTable,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::played_answers::{
    Pattern, ReadAnswer, answer_on, next_of, pattern_hash, snapshot,
};
use crate::utils::played_cluster::{EPOCH, Partition, Played, id, run, within};

const P: &str = "http://ex.org/p";

/// An IRI of about `bytes` bytes, distinct by `n`.
fn long_iri(n: usize, bytes: usize) -> String {
    format!("http://ex.org/{n}/{}", "a".repeat(bytes))
}

/// `partition` holding `<subject> <p> each of objects`, the IRIs by IDs from `base`; the predicate's ID is 1.
fn hold(
    partition: &Partition,
    subject: &str,
    objects: &[String],
    base: u64,
) -> TestResult<SystemId> {
    let predicate = id(1)?;
    partition.term(P, predicate);
    let s = id(base)?;
    partition.term(subject, s);
    let mut held = Vec::with_capacity(objects.len());
    for (at, object) in (base + 1..).zip(objects) {
        let o = id(at)?;
        partition.term(object, o);
        held.push(EncodedTerm::id(o));
    }
    partition.quads(Graph::Default, s, predicate, &held);
    Ok(s)
}

/// `partition` holding `<http://ex.org/s{i}> <p> object` for each object, each subject a list of its own.
fn hold_rows(partition: &Partition, objects: &[String]) -> TestResult {
    for (at, object) in (0u64..).zip(objects) {
        let _subject = hold(
            partition,
            &format!("http://ex.org/s{at}"),
            std::slice::from_ref(object),
            10 + at * 10,
        )?;
    }
    Ok(())
}

/// The answer to `read` over one partition `fill` fills.
async fn answer(
    fill: impl FnOnce(&Partition) -> TestResult,
    read: &ultragraphdb_protocol_wire::ComputeRead,
) -> TestResult<(Played, ReadAnswer)> {
    let played = Played::start(1, read_times_of(None, None)).await?;
    fill(played.partition(1)?)?;
    let mut client = played.client().await?;
    let answered = answer_on(&mut client, read).await?;
    Ok((played, answered))
}

/// How many rows each batch holds.
fn shape(answered: &ReadAnswer) -> Vec<usize> {
    answered
        .batches
        .iter()
        .map(|batch| batch.rows.len())
        .collect()
}

/// A read whose rows fit one frame is answered by one batch, flagged last, naming the epoch it read at and the one partition it
/// reached; the read then ends, holding nothing open.
#[test]
fn test_a_short_read_is_one_last_batch() -> TestResult {
    run(async {
        let objects: Vec<String> = (0..3).map(|i| format!("http://ex.org/o{i}")).collect();
        let read = snapshot(&Pattern::of(None, Some(P), None))?;
        let (played, answered) = answer(|partition| hold_rows(partition, &objects), &read).await?;
        assert_eq!(answered.whole()?.len(), 3);
        assert_eq!(shape(&answered), [3]);
        let batch = answered.batches.first().context("a batch")?;
        assert_eq!((batch.epoch, batch.partition), (EPOCH, 1));
        within(Duration::from_secs(5), "the read's end", async {
            while played.open_reads() > 0 {
                compio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok(())
        })
        .await
    })
}

/// Rows of 200,000 bytes each: two fit a frame of 524,288 bytes, three do not, so four rows go out two by two, each batch within
/// the client's frame.
#[test]
fn test_rows_past_a_frame_go_in_the_next_batch() -> TestResult {
    run(async {
        let objects: Vec<String> = (0..4).map(|i| long_iri(i, 200_000)).collect();
        let read = snapshot(&Pattern::of(None, Some(P), None))?;
        let (_played, answered) = answer(|partition| hold_rows(partition, &objects), &read).await?;
        assert_eq!(answered.whole()?.len(), 4);
        assert_eq!(shape(&answered), [2, 2]);
        assert!(
            answered
                .batches
                .iter()
                .all(|batch| batch.size <= MAX_FRAME_FLOOR)
        );
        Ok(())
    })
}

/// A row larger than the client's frame refuses the read with code 6, `detail` the frame's size, before anything else is sent.
#[test]
fn test_a_row_too_large_for_any_frame_refuses_the_read() -> TestResult {
    run(async {
        let objects = vec![long_iri(0, 600_000)];
        let read = snapshot(&Pattern::of(None, Some(P), None))?;
        let (_played, answered) = answer(|partition| hold_rows(partition, &objects), &read).await?;
        assert!(answered.batches.is_empty(), "{:?}", shape(&answered));
        let refused = answered.refusal()?;
        assert_eq!(refused.code, ErrorCode::TooLarge);
        assert_eq!(refused.detail, MAX_FRAME_FLOOR as u64);
        Ok(())
    })
}

/// A row whose terms share a string fits a frame by the frame it builds, where the string is held once, though counting the
/// string once per term puts it past the frame: its predicate's IRI of 300,000 bytes is its object's datatype too.
#[test]
fn test_a_row_whose_terms_share_a_string_fits_by_its_frame() -> TestResult {
    run(async {
        let datatype = long_iri(0, 300_000);
        let read = snapshot(&Pattern::of(Some("http://ex.org/s"), None, None))?;
        let (_played, answered) = answer(
            |partition| {
                let (s, p) = (id(1)?, id(2)?);
                partition.term("http://ex.org/s", s);
                partition.term(&datatype, p);
                let literal = Literal::new_typed_literal("x", NamedNode::new(datatype.as_str())?);
                let object =
                    EncodedTerm::literal(literal.as_ref(), |_| Ok::<_, TermError>(p))?.into_term();
                partition.quads(Graph::Default, s, p, &[object]);
                partition.put(
                    ultragraphdb_core_keys::QuadKey::SubjectPredicates {
                        tenant: crate::utils::played_cluster::TENANT,
                        graph: Graph::Default,
                        subject: s,
                    }
                    .to_bytes(),
                    crate::utils::played_cluster::list_of(&[EncodedTerm::id(p)]),
                );
                Ok(())
            },
            &read,
        )
        .await?;
        let rows = answered.whole()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows.first().and_then(|row| row.first()).cloned().flatten(),
            Some(datatype)
        );
        Ok(())
    })
}

/// A limit's resume key that does not fit beside the rows held goes in a last batch of its own, holding no row.
#[test]
fn test_a_resume_key_not_fitting_beside_the_rows_ends_the_read_alone() -> TestResult {
    run(async {
        let objects = vec![long_iri(0, 300_000), "http://ex.org/o1".to_string()];
        let read = snapshot(&Pattern::of(None, Some(P), None))?.with_limit(1);
        let (_played, answered) = answer(|partition| hold_rows(partition, &objects), &read).await?;
        assert_eq!(answered.whole()?.len(), 1);
        assert_eq!(shape(&answered), [1, 0]);
        let first = answered.batches.first().context("the rows' batch")?;
        assert!(!first.last && first.resume.is_empty());
        let last = answered.batches.last().context("the last batch")?;
        assert!(last.last && !last.resume.is_empty());
        assert!(
            answered
                .batches
                .iter()
                .all(|batch| batch.size <= MAX_FRAME_FLOOR)
        );
        Ok(())
    })
}

/// A resume key larger than the client's frame, its bound subject's text and its row's object's past 524,288 bytes together,
/// refuses the read with code 6, `detail` the frame's size.
#[test]
fn test_a_resume_key_too_large_for_any_frame_refuses_the_read() -> TestResult {
    run(async {
        let subject = long_iri(0, 300_000);
        let objects = vec![long_iri(1, 300_000), "http://ex.org/o1".to_string()];
        let read = snapshot(&Pattern::of(Some(&subject), Some(P), None))?.with_limit(1);
        let (_played, answered) = answer(
            |partition| hold(partition, &subject, &objects, 10).map(|_| ()),
            &read,
        )
        .await?;
        let refused = answered.refusal()?;
        assert_eq!(refused.code, ErrorCode::TooLarge);
        assert_eq!(refused.detail, MAX_FRAME_FLOOR as u64);
        Ok(())
    })
}

/// A read waits for its client's credit between batches, holding its read open, and ends once its last batch has gone.
#[test]
fn test_a_read_holds_its_batches_to_its_clients_credit() -> TestResult {
    run(async {
        let played = Played::start(1, read_times_of(None, None)).await?;
        let objects: Vec<String> = (0..4).map(|i| long_iri(i, 200_000)).collect();
        hold_rows(played.partition(1)?, &objects)?;
        let mut client = played.client().await?;
        let read = snapshot(&Pattern::of(None, Some(P), None))?
            .with_client_limits(MAX_FRAME_FLOOR as u32, 1);
        client.send_only(&read).await?;
        let stream = client.last_stream();
        let first = next_of(&mut client)
            .await?
            .ok()
            .context("the first batch")?;
        assert!(!first.last);
        compio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(played.open_reads(), 1, "the read waits for credit");
        client.send_on(stream, &Credit::new(1)).await?;
        let last = next_of(&mut client).await?.ok().context("the last batch")?;
        assert!(last.last);
        within(Duration::from_secs(5), "the read's end", async {
            while played.open_reads() > 0 {
                compio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok(())
        })
        .await
    })
}

/// An object IRI `http://ex.org/o/aaa…` whose size `size_of` puts at exactly `target`, found by halving: a size never shrinks as
/// the IRI grows.
fn object_of_size(
    target: usize,
    size_of: impl Fn(&str) -> TestResult<usize>,
) -> TestResult<String> {
    let object = |length: usize| format!("http://ex.org/o/{}", "a".repeat(length));
    let (mut low, mut high) = (1usize, target);
    while low < high {
        let middle = low + (high - low) / 2;
        if size_of(&object(middle))? < target {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let found = object(low);
    if size_of(&found)? == target {
        Ok(found)
    } else {
        Err(ultragraphdb_tool_bdd::TestError::new(format!(
            "no object puts the frame at {target} bytes: {} bytes take it to {}",
            low,
            size_of(&found)?
        )))
    }
}

/// The batch a read of `?s <p> ?o` at [`EPOCH`] on partition 1 starts, as the wire builds it: three columns, the subject, the object
/// and the reifier.
fn empty_batch() -> ResultBatch {
    ResultBatch::new(3).with_read_at(EPOCH, 1)
}

/// A row that brings its batch to exactly the client's frame, as the batch counts it, joins it: two rows go out in one last batch.
#[test]
fn test_a_batch_fills_to_exactly_the_clients_frame() -> TestResult {
    run(async {
        let first = ("http://ex.org/s0", "http://ex.org/o0");
        let mut batch = empty_batch();
        let (s, o) = (ResultTerm::iri(first.0), ResultTerm::iri(first.1));
        batch.push_row(&[Some(&s), Some(&o), None])?;
        let object = object_of_size(MAX_FRAME_FLOOR, |object| {
            let (subject, row_object) =
                (ResultTerm::iri("http://ex.org/s1"), ResultTerm::iri(object));
            Ok(batch.frame_size_with(&[Some(&subject), Some(&row_object), None], 0))
        })?;
        let objects = vec![first.1.to_string(), object];
        let read = snapshot(&Pattern::of(None, Some(P), None))?;
        let (_played, answered) = answer(|partition| hold_rows(partition, &objects), &read).await?;
        assert_eq!(answered.whole()?.len(), 2);
        assert_eq!(shape(&answered), [2]);
        Ok(())
    })
}

/// A resume key that brings an empty last batch to exactly the client's frame goes out in it.
#[test]
fn test_a_resume_key_fills_its_last_batch_to_exactly_the_clients_frame() -> TestResult {
    run(async {
        let read = snapshot(&Pattern::of(None, Some(P), None))?.with_limit(1);
        let hash = pattern_hash(&read)?;
        let resume_of = |object: &str| -> TestResult<Vec<u8>> {
            let mut table = TermTable::new();
            let subject = table.iri("http://ex.org/s0");
            let predicate = table.iri(P);
            let object = table.intern(ultragraphdb_protocol_wire::RequestTerm::Iri(
                object.to_string(),
            ))?;
            let row = ResumeRow::new(None, subject, predicate, object, None);
            Ok(ResumeKey::new(hash, read.read_version(), table, row).encode())
        };
        let object = object_of_size(MAX_FRAME_FLOOR, |object| {
            Ok(empty_batch().frame_size_with(&[], resume_of(object)?.len()))
        })?;
        let objects = vec![object.clone(), "http://ex.org/o1".to_string()];
        let (_played, answered) = answer(|partition| hold_rows(partition, &objects), &read).await?;
        assert_eq!(answered.whole()?.len(), 1);
        assert_eq!(shape(&answered), [1, 0]);
        let last = answered.batches.last().context("the last batch")?;
        assert_eq!(last.resume, resume_of(&object)?);
        Ok(())
    })
}
