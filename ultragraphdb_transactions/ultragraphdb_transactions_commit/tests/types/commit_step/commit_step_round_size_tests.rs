/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A round's size counts what a transaction's commit-time operations leave (commit-rounds): the record they write
//! again and the dictionary entries of the terms they create, so a round never holds a body past its bound, and a
//! transaction whose operations' entries do not fit waits for the next round.

use oxrdf::GraphName;
use ultragraphdb_protocol_wire::{ChunkLayout, HEADER_SIZE};
use ultragraphdb_store_apply::{Applied, Operation};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_sim::{SimListCache, SimStore, SteppedExecutor};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_transactions_commit::{
    ApplierOperator, CommitRequest, CommitStep, Input, Operator, Reply, RoundBounds, StepOutput,
};
use ultragraphdb_transactions_txn::Prepared;

use crate::utils::fixture::{
    Applier, applier, begin, ex, insert, insert_literal, prepare, setup, write,
};

type Operating<'a> =
    ApplierOperator<'a, SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;

/// The bytes the dictionary entry of `key` and `value` takes in a Round body: its two lengths, key and value, padded
/// to 8.
fn entry_len((key, value): (&[u8], &[u8])) -> usize {
    (8 + key.len() + value.len()).div_ceil(8) * 8
}

/// The frame of a Round body committing records of `records` bytes, in order, with `dictionary` bytes of entries: the
/// section table, a decision and a record table entry per record, the records each starting at a multiple of 8, the
/// entries, then the header and the chunk table.
fn frame(records: &[usize], dictionary: usize) -> usize {
    let records_len = records
        .iter()
        .fold(0usize, |end, len| end.div_ceil(8) * 8 + len);
    let body = 32 + 24 * records.len() + records_len.div_ceil(8) * 8 + dictionary;
    HEADER_SIZE + ChunkLayout::of_data(body).body_len()
}

/// The bytes of `prepared`'s dictionary entries in a Round body.
fn dictionary_len(prepared: &Prepared) -> usize {
    prepared.dictionary().map(entry_len).sum()
}

/// The commit step over `applier` with its operator, and what a pipeline does with its outputs: each synced round
/// applied, each released proposal released, every reply and the frame of every round body kept.
struct Run<'a> {
    applier: &'a Applier,
    step: CommitStep<Prepared>,
    operator: Operating<'a>,
    out: StepOutput<Prepared>,
    frames: Vec<usize>,
    replies: Vec<Reply>,
}

impl<'a> Run<'a> {
    fn new(applier: &'a Applier, bounds: RoundBounds) -> Self {
        Self {
            applier,
            step: CommitStep::new(bounds, applier.quads().lists().applied_ts()),
            operator: ApplierOperator::new(applier),
            out: StepOutput::new(),
            frames: Vec::new(),
            replies: Vec::new(),
        }
    }

    fn run(&mut self, input: Input<Prepared>) -> TestResult {
        self.step
            .step_with(input, 0, &mut self.out, &mut self.operator);
        if let Some((_, body)) = self.out.append() {
            self.frames
                .push(HEADER_SIZE + ChunkLayout::of_data(body.len()).body_len());
        }
        let round: Vec<_> = self
            .out
            .apply_mut()
            .drain(..)
            .map(|(version, prepared)| (version, prepared.into_parts().0))
            .collect();
        if !round.is_empty() {
            let _applied: Vec<Applied> = self
                .applier
                .apply_round(round)
                .context("the round applies")?;
        }
        self.replies.extend(self.out.replies().iter().cloned());
        for (_, prepared) in self.out.released_mut().drain(..) {
            let _buffers = prepared.release(self.applier);
        }
        Ok(())
    }
}

/// A read-write transaction writing `changes` and asking `operation`, as its proposal.
fn proposal(
    applier: &Applier,
    changes: Vec<ultragraphdb_core_mapping::Change>,
    operation: Option<Operation>,
) -> TestResult<Prepared> {
    let mut txn = begin(applier)?;
    write(&mut txn, changes)?;
    if let Some(operation) = operation {
        txn.operate(operation, 0).context("asked")?;
    }
    prepare(txn)
}

/// The record and the bytes of dictionary entries `operating`'s operations leave, measured on an operator of its own
/// and let go.
fn operated(applier: &Applier, operating: &Prepared) -> TestResult<(usize, usize)> {
    let mut measure: Operating<'_> = ApplierOperator::new(applier);
    let staged = measure
        .operate(operating, std::iter::empty(), &mut Vec::new())
        .context("operated")?;
    let record = measure.staged_record(&staged).len();
    let entries = measure.staged_dictionary(&staged).map(entry_len).sum();
    measure.discard(operating, staged);
    Ok((record, entries))
}

#[test]
fn test_a_round_counts_the_entries_operations_add_and_holds_no_body_past_its_bound() -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let opener = proposal(&applier, vec![insert("x", "knows", "b")], None)?;
    // A plain commit with a long literal, whose record and string entry fill most of the round.
    let plain = proposal(
        &applier,
        vec![insert_literal("p", "note", &"n".repeat(3_000))],
        None,
    )?;
    // A commit counting on a subject named by a long new IRI: its operation creates the IRI, whose entries the round's
    // dictionary holds.
    let count = Operation::CounterAdd {
        graph: GraphName::DefaultGraph,
        subject: ex(&"l".repeat(2_000)).into(),
        predicate: ex("n"),
        delta: 1,
    };
    let operating = proposal(&applier, vec![insert("o", "knows", "b")], Some(count))?;
    let (operated_record, operated_entries) = operated(&applier, &operating)?;
    // The bound holds the two with the record the operation leaves and the entries before it: the entries it adds
    // overflow the round, and a round of its own holds it.
    let records = [plain.record().len(), operated_record];
    let bound = frame(
        &records,
        dictionary_len(&plain) + dictionary_len(&operating),
    );
    assert!(
        operated_entries >= dictionary_len(&operating) + 2_000,
        "the operation adds the long IRI's entries"
    );
    assert!(frame(&records, dictionary_len(&plain) + operated_entries) > bound);
    assert!(frame(&[operated_record], operated_entries) <= bound);
    let bounds = RoundBounds::new(
        u32::try_from(bound).context("a bound within u32")?,
        u32::MAX,
    );
    let mut run = Run::new(&applier, bounds);
    // The opener's round is in flight while the two wait; they are decided together once it is synced.
    run.run(Input::Commit(CommitRequest::new(1, opener)))?;
    run.run(Input::Commit(CommitRequest::new(2, plain)))?;
    run.run(Input::Commit(CommitRequest::new(3, operating)))?;
    for _ in 0..3 {
        run.run(Input::Synced)?;
    }
    assert_eq!(
        run.frames.len(),
        3,
        "the operated commit waits for a round of its own"
    );
    assert!(
        run.frames.iter().all(|f| *f <= bound),
        "no round past its bound of {bound}: {:?}",
        run.frames
    );
    let outcomes: Vec<(u64, bool)> = run
        .replies
        .iter()
        .map(|r| (r.ticket(), r.outcome().is_ok()))
        .collect();
    assert_eq!(outcomes, vec![(1, true), (2, true), (3, true)]);
    Ok(())
}
