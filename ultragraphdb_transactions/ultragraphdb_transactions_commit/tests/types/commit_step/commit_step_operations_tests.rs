/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Commit-time operations in the step (7.5): run by the step's operator after the resolver admits a transaction and
//! before it takes a version, over the decided but unapplied proposals; their record is what the history holds and the
//! round logs.

use ultragraphdb_protocol_wire::{ConflictKind, OperationResult, WriteSetRecord};
use ultragraphdb_traits_store::VersionlessEntry;
use ultragraphdb_transactions_commit::{
    CommitError, CommitRequest, CommitStep, Input, NoOperations, Operator,
    Proposal, Reply, RoundBounds, SessionEntry, StepOutput,
};
use ultragraphdb_transactions_txn::ReadSet;

use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal, proposal_in, read_set};

/// A proposal that asks for an operation when `asks` holds.
#[derive(Debug, Clone)]
struct OpProposal {
    inner: TestProposal,
    asks: bool,
}

impl Proposal for OpProposal {
    fn read_set(&self) -> &ReadSet {
        &self.inner.read_set
    }

    fn record(&self) -> &[u8] {
        &self.inner.record
    }

    fn has_operations(&self) -> bool {
        self.asks
    }
}

/// An operator by script: an operation writes the conflict point `key` and returns the result `uid`, or is refused
/// when `refuse` holds; `padding` bytes of commit record are added to the record it leaves.
#[derive(Debug, Default)]
struct Script {
    key: Vec<u8>,
    uid: u64,
    refuse: bool,
    padding: usize,
    rounds: usize,
    admitted: usize,
    /// The number of unapplied proposals each operation saw.
    seen: Vec<usize>,
}

impl Operator<OpProposal> for Script {
    type Staged = Vec<u8>;

    fn begin_round(&mut self) {
        self.rounds += 1;
    }

    fn operate<'p, I>(
        &mut self,
        _proposal: &OpProposal,
        unapplied: I,
        results: &mut Vec<OperationResult>,
    ) -> Result<Vec<u8>, CommitError>
    where
        I: Iterator<Item = &'p OpProposal>,
    {
        self.seen.push(unapplied.count());
        if self.refuse {
            return Err(CommitError::OperationFailed(Some("\"done\"".to_string())));
        }
        results.push(OperationResult::new(self.uid, true));
        let mut record = WriteSetRecord::new(0, 0, 0, 0);
        record.push_conflict_point(self.key.clone());
        record.set_commit_record(vec![1; self.padding]);
        Ok(record.encode())
    }

    fn staged_record<'s>(&self, staged: &'s Vec<u8>) -> &'s [u8] {
        staged
    }

    fn adopt(&mut self, proposal: &mut OpProposal, staged: Vec<u8>) {
        proposal.inner.record = staged;
    }

    fn admitted(&mut self, _proposal: &OpProposal) {
        self.admitted += 1;
    }
}

type Step = CommitStep<OpProposal>;
type Out = StepOutput<OpProposal>;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

fn plain(read_version: u64, reads: &Reads, writes: &Writes) -> OpProposal {
    OpProposal {
        inner: proposal(read_version, reads, writes),
        asks: false,
    }
}

fn asking(writes: &Writes) -> OpProposal {
    OpProposal {
        inner: proposal(0, &Reads::default(), writes),
        asks: true,
    }
}

fn commit(step: &mut Step, out: &mut Out, ticket: u64, p: OpProposal, script: &mut Script) {
    step.step_with(Input::Commit(CommitRequest::new(ticket, p)), 0, out, script);
}

fn sync(step: &mut Step, out: &mut Out, script: &mut Script) {
    step.step_with(Input::Synced, 0, out, script);
}

#[test]
fn test_a_refused_operation_takes_no_version_and_adds_nothing_to_the_history() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    let mut script = Script {
        key: b"op".to_vec(),
        refuse: true,
        ..Script::default()
    };
    commit(
        &mut step,
        &mut out,
        1,
        plain(0, &Reads::default(), &Writes::point(b"a")),
        &mut script,
    );
    commit(
        &mut step,
        &mut out,
        2,
        asking(&Writes::point(b"b")),
        &mut script,
    );
    commit(
        &mut step,
        &mut out,
        3,
        plain(0, &Reads::default(), &Writes::point(b"c")),
        &mut script,
    );
    sync(&mut step, &mut out, &mut script);
    // Round 1: ticket 1 at 2. Round 2, closed now: ticket 2 refused and released, ticket 3 at 4, the version the
    // refusal took none of.
    assert_eq!(out.released().len(), 1, "the refused proposal is released");
    sync(&mut step, &mut out, &mut script);
    let reply = |t: u64| {
        out.replies()
            .iter()
            .find(|r| r.ticket() == t)
            .cloned()
            .expect("answered")
    };
    assert_eq!(
        reply(2).outcome(),
        &Err(CommitError::OperationFailed(Some("\"done\"".to_string())))
    );
    assert!(reply(2).results().is_empty());
    assert_eq!(reply(3).outcome(), &Ok(4));
    assert_eq!(
        script.admitted, 2,
        "only the two admitted reach the operator"
    );
    // A read of the refused operation's key below its round is no conflict: nothing of it is in the history.
    script.refuse = false;
    commit(
        &mut step,
        &mut out,
        4,
        plain(2, &Reads::list(b"op"), &Writes::point(b"d")),
        &mut script,
    );
    sync(&mut step, &mut out, &mut script);
    assert_eq!(out.replies()[0].outcome(), &Ok(6));
}

#[test]
fn test_the_record_an_operation_leaves_is_logged_held_and_answered_with_its_results() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    let mut script = Script {
        key: b"node".to_vec(),
        uid: 77,
        padding: 40,
        ..Script::default()
    };
    commit(
        &mut step,
        &mut out,
        1,
        asking(&Writes::point(b"a")),
        &mut script,
    );
    let (_, body) = out.append().expect("a round closed");
    let mut staged = WriteSetRecord::new(0, 0, 0, 0);
    staged.push_conflict_point(b"node".to_vec());
    staged.set_commit_record(vec![1; 40]);
    let staged = staged.encode();
    assert!(
        body.windows(staged.len()).any(|w| w == staged.as_slice()),
        "the round logs the record the operation left"
    );
    sync(&mut step, &mut out, &mut script);
    let reply = out.replies()[0].clone();
    assert_eq!(reply.outcome(), &Ok(2));
    assert_eq!(reply.results(), &[OperationResult::new(77, true)]);
    assert_eq!(
        out.apply()[0].1.record(),
        staged.as_slice(),
        "the apply takes it"
    );
    // A transaction that read `node` below version 2 conflicts with the operation's write.
    commit(
        &mut step,
        &mut out,
        2,
        plain(0, &Reads::list(b"node"), &Writes::point(b"z")),
        &mut script,
    );
    sync(&mut step, &mut out, &mut script);
    assert_eq!(
        out.replies()[0].outcome(),
        &Err(CommitError::Conflict(ConflictKind::List))
    );
}

#[test]
fn test_the_operator_sees_the_round_syncing_and_the_rounds_admitted_before_it() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    let mut script = Script {
        key: b"node".to_vec(),
        ..Script::default()
    };
    commit(
        &mut step,
        &mut out,
        1,
        plain(0, &Reads::default(), &Writes::point(b"a")),
        &mut script,
    );
    assert_eq!(script.rounds, 1);
    commit(
        &mut step,
        &mut out,
        2,
        plain(0, &Reads::default(), &Writes::point(b"b")),
        &mut script,
    );
    commit(
        &mut step,
        &mut out,
        3,
        plain(0, &Reads::default(), &Writes::point(b"c")),
        &mut script,
    );
    commit(
        &mut step,
        &mut out,
        4,
        asking(&Writes::point(b"d")),
        &mut script,
    );
    sync(&mut step, &mut out, &mut script);
    // The round syncing (ticket 1) and the two admitted before it in its own round.
    assert_eq!(script.seen, vec![3]);
    assert_eq!(script.rounds, 2);
    commit(
        &mut step,
        &mut out,
        5,
        asking(&Writes::point(b"e")),
        &mut script,
    );
    sync(&mut step, &mut out, &mut script);
    // Round 2 applied by now: the round syncing, of three, and nothing admitted before it.
    assert_eq!(script.seen, vec![3, 3]);
}

#[test]
fn test_an_operation_whose_record_no_longer_fits_waits_for_the_next_round() {
    let mut script = Script {
        key: b"node".to_vec(),
        padding: 3_500,
        ..Script::default()
    };
    let small = || plain(0, &Reads::default(), &Writes::point(b"a"));
    // A round of 4 KiB holds two small records, or the operation's record of 3.5 KiB alone, never both.
    let mut step = Step::new(RoundBounds::new(4_096, 16), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, small(), &mut script);
    commit(&mut step, &mut out, 2, small(), &mut script);
    commit(&mut step, &mut out, 3, small(), &mut script);
    commit(
        &mut step,
        &mut out,
        4,
        asking(&Writes::point(b"b")),
        &mut script,
    );
    sync(&mut step, &mut out, &mut script);
    let (round, _) = out.append().expect("the second round");
    assert_eq!(round.transactions(), 2, "the operation waits");
    assert_eq!(step.waiting(), 1);
    sync(&mut step, &mut out, &mut script);
    let (round, _) = out.append().expect("the third round");
    assert_eq!(round.transactions(), 1);
    sync(&mut step, &mut out, &mut script);
    // 1 alone at 2; 2 and 3 at 4 and 6; the operation, decided again, alone at 8 with its result.
    assert_eq!(
        out.replies(),
        &[Reply::new(4, Ok(8)).with_results(vec![OperationResult::new(0, true)])]
    );
    assert_eq!(script.seen.len(), 2, "decided again in the next round");
}

#[test]
fn test_a_step_without_an_operator_refuses_operations() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    step.step(
        Input::Commit(CommitRequest::new(1, asking(&Writes::point(b"a")))),
        0,
        &mut out,
    );
    step.step(Input::Synced, 0, &mut out);
    assert_eq!(
        out.replies(),
        &[Reply::new(
            1,
            Err(CommitError::Refused(
                "the commit step runs no commit-time operations".to_string()
            ))
        )]
    );
    let mut none = NoOperations;
    let staged = ();
    assert!(<NoOperations as Operator<OpProposal>>::staged_record(&none, &staged).is_empty());
    <NoOperations as Operator<OpProposal>>::begin_round(&mut none);
    let _ = read_set(0, 0, &Reads::default());
}

/// A retry of a request whose operations ran is answered with its results as the commit was, from the table, whose
/// entry the step hands to the apply with the round (7.8).
#[test]
fn test_a_retry_is_answered_with_its_results_and_the_tables_entry_holds_them() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    let mut script = Script {
        key: b"node".to_vec(),
        uid: 77,
        ..Script::default()
    };
    step.step_with(
        Input::Register {
            ticket: 9,
            principal: 0,
        },
        0,
        &mut out,
        &mut script,
    );
    step.step_with(Input::Synced, 0, &mut out, &mut script);
    assert_eq!(out.replies(), &[Reply::new(9, Ok(2))]);
    let index = VersionlessEntry::put(
        SessionEntry::INDEX_KEY.to_vec(),
        2u64.to_le_bytes().to_vec(),
    );
    assert_eq!(
        out.system(),
        &[
            VersionlessEntry::put(
                SessionEntry::key(2),
                SessionEntry::new(0, 0, 0, Vec::new()).encode()
            ),
            index,
        ]
    );
    let asks = || OpProposal {
        inner: proposal_in(2, 1, 0, &Reads::default(), &Writes::point(b"a")),
        asks: true,
    };
    commit(&mut step, &mut out, 1, asks(), &mut script);
    assert!(out.system().is_empty(), "nothing applied yet");
    step.step_with(Input::Synced, 0, &mut out, &mut script);
    let results = vec![OperationResult::new(77, true)];
    assert_eq!(
        out.replies(),
        &[Reply::new(1, Ok(4)).with_results(results.clone())]
    );
    assert_eq!(
        out.system(),
        &[VersionlessEntry::put(
            SessionEntry::key(2),
            SessionEntry::new(1, 4, 0, results.clone()).encode()
        )]
    );
    let operations = script.seen.len();
    commit(&mut step, &mut out, 2, asks(), &mut script);
    assert_eq!(out.replies(), &[Reply::new(2, Ok(4)).with_results(results)]);
    assert_eq!(script.seen.len(), operations, "no operation runs again");
}
