/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Commit-time operations in the step (7.5): run by the step's operator after the resolver admits a transaction and
//! before it takes a version, over the decided but unapplied proposals; their record is what the history holds and the
//! round logs.

use ultragraphdb_protocol_wire::{ConflictKind, MAX_FRAME_FLOOR, OperationResult, WriteSetRecord};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::VersionlessEntry;
use ultragraphdb_transactions_commit::{
    CommitError, CommitRequest, CommitStep, Input, NoOperations, Operator, Proposal, Reply,
    RoundBounds, SessionEntry, StepOutput,
};
use ultragraphdb_transactions_txn::ReadSet;

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_protocol_wire::TermTable;
use ultragraphdb_transactions_commit::StoredReply;

use crate::utils::fixture::version;
use crate::utils::node_operator::{NodeOperator, reply_frame};
use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal, proposal_in, read_set};

/// The term table of a reply whose one result names `http://ex.org/n{uid}`.
fn node_terms(uid: u64) -> TermTable {
    let mut terms = TermTable::new();
    let _node: u32 = terms.iri(&format!("http://ex.org/n{uid}"));
    terms
}

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

    fn is_one_frame(&self) -> bool {
        self.inner.one_frame
    }
}

/// An operator by script: an operation writes the conflict point `key` and returns a result naming the node
/// `http://ex.org/n{uid}`, created, or is refused when `refuse` holds; `padding` bytes of commit record are added to the
/// record it leaves, which is `garbled` bytes that no record reads when those are given.
#[derive(Debug, Default)]
struct Script {
    key: Vec<u8>,
    uid: u64,
    refuse: bool,
    garbled: Option<Vec<u8>>,
    padding: usize,
    rounds: usize,
    admitted: usize,
    /// The number of unapplied proposals each operation saw.
    seen: Vec<usize>,
    /// The staged records the step let go of without adopting them.
    discarded: usize,
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
            return Err(CommitError::operation_failed(Some("\"done\"".to_string())));
        }
        if let Some(garbled) = &self.garbled {
            return Ok(garbled.clone());
        }
        results.push(OperationResult::for_node(0, true));
        let mut record = WriteSetRecord::transaction(0);
        record.push_node_result(&format!("http://ex.org/n{}", self.uid), true);
        record.push_conflict_point(self.key.clone());
        record.set_commit_record(vec![1; self.padding]);
        Ok(record.encode())
    }

    fn staged_record<'s>(&self, staged: &'s Vec<u8>) -> &'s [u8] {
        staged
    }

    fn staged_dictionary<'s>(
        &self,
        _staged: &'s Vec<u8>,
    ) -> impl Iterator<Item = (&'s [u8], &'s [u8])> {
        std::iter::empty()
    }

    fn adopt(&mut self, proposal: &mut OpProposal, staged: Vec<u8>) {
        proposal.inner.record = staged;
    }

    fn discard(&mut self, _proposal: &OpProposal, _staged: Vec<u8>) {
        self.discarded += 1;
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

fn plain(read_version: u64, reads: &Reads, writes: &Writes) -> TestResult<OpProposal> {
    Ok(OpProposal {
        inner: proposal(read_version, reads, writes)?,
        asks: false,
    })
}

fn asking(writes: &Writes) -> TestResult<OpProposal> {
    Ok(OpProposal {
        inner: proposal(0, &Reads::default(), writes)?,
        asks: true,
    })
}

fn commit(step: &mut Step, out: &mut Out, ticket: u64, p: OpProposal, script: &mut Script) {
    step.step_with(Input::Commit(CommitRequest::new(ticket, p)), 0, out, script);
}

fn sync(step: &mut Step, out: &mut Out, script: &mut Script) {
    step.step_with(Input::Synced, 0, out, script);
}

#[test]
fn test_a_refused_operation_takes_no_version_and_adds_nothing_to_the_history() -> TestResult {
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
        plain(0, &Reads::default(), &Writes::point(b"a"))?,
        &mut script,
    );
    commit(
        &mut step,
        &mut out,
        2,
        asking(&Writes::point(b"b"))?,
        &mut script,
    );
    commit(
        &mut step,
        &mut out,
        3,
        plain(0, &Reads::default(), &Writes::point(b"c"))?,
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
            .context("answered")
    };
    assert_eq!(
        reply(2)?.outcome(),
        &Err(CommitError::operation_failed(Some("\"done\"".to_string())))
    );
    assert!(reply(2)?.results().is_empty());
    assert_eq!(reply(3)?.outcome(), &Ok(version(1, 2)));
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
        plain(2, &Reads::list(b"op"), &Writes::point(b"d"))?,
        &mut script,
    );
    sync(&mut step, &mut out, &mut script);
    assert_eq!(out.replies()[0].outcome(), &Ok(version(1, 4)));
    Ok(())
}

/// Operations leaving a record that does not read are refused malformed, naming why it does not, and what they left is
/// let go; the round goes on with the next transaction.
#[test]
fn test_an_operation_leaving_a_record_that_does_not_read_is_refused_malformed() -> TestResult {
    use ultragraphdb_protocol_wire::WriteSetRecordView;
    let garbled = vec![0xFF; 3];
    let cause = WriteSetRecordView::new(&garbled)
        .err()
        .context("three bytes are no record")?;
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    let mut script = Script {
        key: b"op".to_vec(),
        garbled: Some(garbled),
        ..Script::default()
    };
    let mut answered = Vec::new();
    commit(
        &mut step,
        &mut out,
        1,
        asking(&Writes::point(b"b"))?,
        &mut script,
    );
    answered.extend(out.replies().iter().cloned());
    commit(
        &mut step,
        &mut out,
        2,
        plain(0, &Reads::default(), &Writes::point(b"c"))?,
        &mut script,
    );
    answered.extend(out.replies().iter().cloned());
    for _ in 0..2 {
        sync(&mut step, &mut out, &mut script);
        answered.extend(out.replies().iter().cloned());
    }
    let outcome = |t: u64| {
        answered
            .iter()
            .find(|r| r.ticket() == t)
            .map(|r| r.outcome().clone())
            .context("answered")
    };
    assert_eq!(outcome(1)?, Err(CommitError::malformed(cause)));
    assert!(outcome(2)?.is_ok());
    assert_eq!(script.discarded, 1, "what the operations left is let go");
    Ok(())
}

#[test]
fn test_the_record_an_operation_leaves_is_logged_held_and_answered_with_its_results() -> TestResult
{
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
        asking(&Writes::point(b"a"))?,
        &mut script,
    );
    let (_, body) = out.append().context("a round closed")?;
    let mut staged = WriteSetRecord::transaction(0);
    staged.push_node_result("http://ex.org/n77", true);
    staged.push_conflict_point(b"node".to_vec());
    staged.set_commit_record(vec![1; 40]);
    let staged = staged.encode();
    assert!(
        body.windows(staged.len()).any(|w| w == staged.as_slice()),
        "the round logs the record the operation left"
    );
    sync(&mut step, &mut out, &mut script);
    let reply = out.replies()[0].clone();
    assert_eq!(reply.outcome(), &Ok(version(1, 0)));
    assert_eq!(reply.results(), &[OperationResult::for_node(0, true)]);
    assert_eq!(reply.terms(), &node_terms(77));
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
        plain(0, &Reads::list(b"node"), &Writes::point(b"z"))?,
        &mut script,
    );
    sync(&mut step, &mut out, &mut script);
    assert_eq!(
        out.replies()[0].outcome(),
        &Err(CommitError::conflict(ConflictKind::List))
    );
    Ok(())
}

#[test]
fn test_the_operator_sees_the_round_syncing_and_the_rounds_admitted_before_it() -> TestResult {
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
        plain(0, &Reads::default(), &Writes::point(b"a"))?,
        &mut script,
    );
    assert_eq!(script.rounds, 1);
    commit(
        &mut step,
        &mut out,
        2,
        plain(0, &Reads::default(), &Writes::point(b"b"))?,
        &mut script,
    );
    commit(
        &mut step,
        &mut out,
        3,
        plain(0, &Reads::default(), &Writes::point(b"c"))?,
        &mut script,
    );
    commit(
        &mut step,
        &mut out,
        4,
        asking(&Writes::point(b"d"))?,
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
        asking(&Writes::point(b"e"))?,
        &mut script,
    );
    sync(&mut step, &mut out, &mut script);
    // Round 2 applied by now: the round syncing, of three, and nothing admitted before it.
    assert_eq!(script.seen, vec![3, 3]);
    Ok(())
}

#[test]
fn test_an_operation_whose_record_no_longer_fits_waits_for_the_next_round() -> TestResult {
    let mut script = Script {
        key: b"node".to_vec(),
        padding: 3_500,
        ..Script::default()
    };
    let small = || plain(0, &Reads::default(), &Writes::point(b"a"));
    // A round of 4 KiB holds two small records, or the operation's record of 3.5 KiB alone, never both.
    let mut step = Step::new(RoundBounds::new(4_096, 16), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, small()?, &mut script);
    commit(&mut step, &mut out, 2, small()?, &mut script);
    commit(&mut step, &mut out, 3, small()?, &mut script);
    commit(
        &mut step,
        &mut out,
        4,
        asking(&Writes::point(b"b"))?,
        &mut script,
    );
    sync(&mut step, &mut out, &mut script);
    let (round, _) = out.append().context("the second round")?;
    assert_eq!(round.transactions(), 2, "the operation waits");
    assert_eq!(step.waiting(), 1);
    sync(&mut step, &mut out, &mut script);
    let (third_round, _) = out.append().context("the third round")?;
    assert_eq!(third_round.transactions(), 1);
    sync(&mut step, &mut out, &mut script);
    // 1 alone at 2; 2 and 3 at 4 and 6; the operation, decided again, alone at 8 with its result.
    assert_eq!(
        crate::utils::fixture::plain(out.replies()),
        &[Reply::new(4, Ok(version(1, 6)))
            .with_results(vec![OperationResult::for_node(0, true)], node_terms(0))]
    );
    assert_eq!(
        script.seen.len(),
        2,
        "decided again in the next third_round"
    );
    Ok(())
}

#[test]
fn test_a_step_without_an_operator_refuses_operations() -> TestResult {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    step.step(
        Input::Commit(CommitRequest::new(1, asking(&Writes::point(b"a"))?)),
        0,
        &mut out,
    );
    step.step(Input::Synced, 0, &mut out);
    assert_eq!(
        crate::utils::fixture::plain(out.replies()),
        &[Reply::new(
            1,
            Err(CommitError::refused(
                "the commit step runs no commit-time operations".to_string()
            ))
        )]
    );
    let mut none = NoOperations;
    let staged = ();
    assert!(<NoOperations as Operator<OpProposal>>::staged_record(&none, &staged).is_empty());
    <NoOperations as Operator<OpProposal>>::begin_round(&mut none);
    let _read_set: ReadSet = read_set(0, 0, &Reads::default())?;
    Ok(())
}

/// A retry of a request whose operations ran is answered with its results as the commit was, from the table, whose
/// entry the step hands to the apply with the round (7.8).
#[test]
fn test_a_retry_is_answered_with_its_results_and_the_tables_entry_holds_them() -> TestResult {
    let mut step = Step::new(unbounded(), 0).with_partition(1);
    let mut out = Out::new();
    let mut script = Script {
        key: b"node".to_vec(),
        uid: 77,
        ..Script::default()
    };
    let session = 2u128;
    let asks = || -> TestResult<OpProposal> {
        Ok(OpProposal {
            inner: proposal_in(session, 1, 0, &Reads::default(), &Writes::point(b"a"))?,
            asks: true,
        })
    };
    commit(&mut step, &mut out, 1, asks()?, &mut script);
    assert!(out.system().is_empty(), "nothing applied yet");
    step.step_with(Input::Synced, 0, &mut out, &mut script);
    let results = vec![OperationResult::for_node(0, true)];
    let token = SessionToken::new(1, 1, Epoch::new(1).context("the epoch")?);
    assert_eq!(
        out.replies(),
        &[Reply::new(1, Ok(version(1, 0)))
            .with_token(token.clone())
            .with_results(results.clone(), node_terms(77))]
    );
    let stored = StoredReply::new(
        version(1, 0),
        token.clone(),
        results.clone(),
        node_terms(77),
    );
    let index = VersionlessEntry::put(
        SessionEntry::INDEX_KEY.to_vec(),
        session.to_be_bytes().to_vec(),
    );
    // The round's applied log position closes its entries: position 1, 8 bytes little-endian.
    let applied_position = VersionlessEntry::put(
        ultragraphdb_transactions_commit::Replay::APPLIED_POSITION_KEY.to_vec(),
        1u64.to_le_bytes().to_vec(),
    );
    assert_eq!(
        out.system(),
        &[
            VersionlessEntry::put(
                SessionEntry::key(session),
                SessionEntry::new(1, stored).encode()
            ),
            index,
            applied_position,
        ]
    );
    let operations = script.seen.len();
    let resend = OpProposal {
        inner: asks()?.inner.with_one_frame(),
        asks: true,
    };
    commit(&mut step, &mut out, 2, resend, &mut script);
    assert_eq!(
        out.replies(),
        &[Reply::new(2, Ok(version(1, 0)))
            .with_token(token)
            .with_results(results, node_terms(77))]
    );
    assert_eq!(script.seen.len(), operations, "no operation runs again");
    Ok(())
}

/// The record `script`'s operation leaves, built as `Script::operate` builds it.
fn staged(script: &Script) -> Vec<u8> {
    let mut record = WriteSetRecord::transaction(0);
    record.push_node_result(&format!("http://ex.org/n{}", script.uid), true);
    record.push_conflict_point(script.key.clone());
    record.set_commit_record(vec![1; script.padding]);
    record.encode()
}

/// An operation whose record no round holds, even one of its own, is refused too large when it is decided: no
/// version, nothing in the history, its record not adopted, released and answered with its round, and the commit behind
/// it decided next. Without the refusal it was put back at the head of every round, each closing empty, forever.
#[test]
fn test_an_operation_whose_record_fits_no_round_is_refused_too_large_and_the_queue_moves_on()
-> TestResult {
    use crate::utils::frames::{largest_record, round_frame};
    let mut script = Script {
        key: b"node".to_vec(),
        padding: 5_000,
        ..Script::default()
    };
    let size = staged(&script).len();
    assert!(round_frame(&[staged(&script)]).is_some_and(|f| f > 4_096));
    let mut step = Step::new(RoundBounds::new(4_096, 16), 0);
    let mut out = Out::new();
    let op = asking(&Writes::point(b"a"))?;
    let original = op.inner.record.clone();
    commit(&mut step, &mut out, 1, op, &mut script);
    // Decided in the round it closed at once: an abort, no version (max(0 + 2, 0) would have been 2).
    assert_eq!(
        out.append().map(|(r, _)| r),
        Some(ultragraphdb_transactions_commit::RoundRecord::new(
            version(1, 0),
            1,
            0
        ))
    );
    let released: Vec<(u64, Vec<u8>)> = out
        .released()
        .iter()
        .map(|(t, p)| (*t, p.inner.record.clone()))
        .collect();
    assert_eq!(
        released,
        vec![(1, original)],
        "released with its own record"
    );
    commit(
        &mut step,
        &mut out,
        2,
        plain(0, &Reads::default(), &Writes::point(b"b"))?,
        &mut script,
    );
    sync(&mut step, &mut out, &mut script);
    assert_eq!(
        crate::utils::fixture::plain(out.replies()),
        &[Reply::new(
            1,
            Err(CommitError::too_large(size, largest_record(4_096)))
        )]
    );
    assert_eq!(
        out.append().map(|(r, _)| r),
        Some(ultragraphdb_transactions_commit::RoundRecord::new(
            version(1, 0),
            1,
            1
        ))
    );
    sync(&mut step, &mut out, &mut script);
    assert_eq!(
        crate::utils::fixture::plain(out.replies()),
        &[Reply::new(2, Ok(version(1, 0)))]
    );
    assert_eq!(script.seen.len(), 1, "decided once");
    assert_eq!(script.admitted, 1, "the plain commit alone");
    assert_eq!(
        step.resolver()
            .check(&read_set(0, 0, &Reads::list(b"node"))?),
        Ok(()),
        "nothing of the operation in the history"
    );
    Ok(())
}

/// The largest operation record a fresh round holds waits for one when the round it is decided in is too full, then
/// commits there; one more byte and no round holds it, so it is refused instead of waiting.
#[test]
fn test_an_operation_record_that_just_fits_a_fresh_round_waits_for_it_and_one_byte_more_is_refused()
-> TestResult {
    use crate::utils::frames::largest_record;
    let padding = largest_fitting_padding()?;
    use ultragraphdb_transactions_commit::RoundRecord;
    // At the capacity: 1 alone at 2; 2 at 4 with the operation deferred; the operation alone at 6.
    let (replies, rounds, _) = run_with_padding(padding)?;
    assert_eq!(
        rounds,
        vec![
            RoundRecord::new(version(1, 0), 1, 1),
            RoundRecord::new(version(1, 2), 1, 1),
            RoundRecord::new(version(1, 4), 1, 1)
        ]
    );
    assert_eq!(
        replies,
        vec![
            Reply::new(1, Ok(version(1, 0))),
            Reply::new(2, Ok(version(1, 2))),
            Reply::new(3, Ok(version(1, 4)))
                .with_results(vec![OperationResult::for_node(0, true)], node_terms(0))
        ]
    );
    // One byte more: refused in the round it is decided in, beside 2.
    let (longer_replies, longer_rounds, size) = run_with_padding(padding + 1)?;
    assert_eq!(
        longer_rounds,
        vec![
            RoundRecord::new(version(1, 0), 1, 1),
            RoundRecord::new(version(1, 2), 2, 1)
        ]
    );
    assert_eq!(
        longer_replies,
        vec![
            Reply::new(1, Ok(version(1, 0))),
            Reply::new(2, Ok(version(1, 2))),
            Reply::new(3, Err(CommitError::too_large(size, largest_record(4_096))))
        ]
    );
    Ok(())
}

/// The largest padding whose operation record a round of 4,096 bytes holds alone, by the wire's builder.
fn largest_fitting_padding() -> TestResult<usize> {
    use crate::utils::frames::round_frame;
    let fits = |padding: usize| {
        round_frame(&[staged(&Script {
            key: b"node".to_vec(),
            padding,
            ..Script::default()
        })])
        .is_some_and(|f| f <= 4_096)
    };
    let padding = (0..4_096)
        .rev()
        .find(|p| fits(*p))
        .context("some record fits")?;
    assert!(!fits(padding + 1));
    Ok(padding)
}

/// Two plain commits and one asking an operation of record padded by `padding`, in rounds of 4,096 bytes: the replies,
/// the rounds appended and the operation record's size.
fn run_with_padding(
    padding: usize,
) -> TestResult<(
    Vec<Reply>,
    Vec<ultragraphdb_transactions_commit::RoundRecord>,
    usize,
)> {
    let small = || plain(0, &Reads::default(), &Writes::point(b"a"));
    let mut script = Script {
        key: b"node".to_vec(),
        padding,
        ..Script::default()
    };
    let mut step = Step::new(RoundBounds::new(4_096, 16), 0);
    let mut out = Out::new();
    let mut replies = Vec::new();
    let mut rounds = Vec::new();
    commit(&mut step, &mut out, 1, small()?, &mut script);
    rounds.extend(out.append().map(|(r, _)| r));
    commit(&mut step, &mut out, 2, small()?, &mut script);
    commit(
        &mut step,
        &mut out,
        3,
        asking(&Writes::point(b"b"))?,
        &mut script,
    );
    for _ in 0..3 {
        sync(&mut step, &mut out, &mut script);
        replies.extend(crate::utils::fixture::plain(out.replies()));
        rounds.extend(out.append().map(|(r, _)| r));
    }
    Ok((replies, rounds, staged(&script).len()))
}

/// A step without an operator, `NoOperations`, refuses a transaction asking commit-time operations, writing nothing,
/// and takes one asking none.
#[test]
fn test_a_step_running_no_operations_refuses_a_transaction_asking_them() -> TestResult {
    use crate::utils::fixture::{applier, begin, ex, insert, prepare, write};
    use ultragraphdb_transactions_commit::{
        CommitError, CommitRequest, CommitStep, Input, StepOutput,
    };
    let applier = applier()?;
    let mut step = CommitStep::new(
        RoundBounds::new(u32::MAX, u32::MAX),
        applier.quads().lists().applied_ts(),
    );
    let mut asking = begin(&applier)?;
    asking.operate(
        ultragraphdb_store_apply::Operation::CounterAdd {
            graph: oxrdf::GraphName::DefaultGraph,
            subject: ex("c").into(),
            predicate: ex("n"),
            delta: 1,
        },
        0,
    )?;
    let mut plain = begin(&applier)?;
    write(&mut plain, vec![insert("a", "knows", "b")])?;
    let mut answered = Vec::new();
    let mut out = StepOutput::new();
    for input in [
        Input::Commit(CommitRequest::new(1, prepare(asking)?)),
        Input::Commit(CommitRequest::new(2, prepare(plain)?)),
        Input::Synced,
        Input::Synced,
    ] {
        step.step(input, 0, &mut out);
        answered.extend(
            out.replies()
                .iter()
                .map(|r| (r.ticket(), r.outcome().is_ok())),
        );
        if let Some(refusal) = out.replies().iter().find(|r| r.ticket() == 1) {
            assert_eq!(
                refusal.outcome(),
                &Err(CommitError::refused(
                    "the commit step runs no commit-time operations".to_string()
                ))
            );
        }
    }
    answered.sort_unstable();
    assert_eq!(answered, vec![(1, false), (2, true)]);
    Ok(())
}

/// The reply the operations leave when its one node takes `iri_len` bytes, as the step answers it: the outcome, the
/// operator's discards and whether the step's newest version moved.
fn decided_with_a_node_of(iri_len: usize) -> TestResult<(Result<u64, CommitError>, usize, bool)> {
    let mut step = CommitStep::<TestProposal>::new(unbounded(), 0);
    let mut out = StepOutput::<TestProposal>::new();
    let mut operator = NodeOperator {
        iri: "n".repeat(iri_len),
        ..NodeOperator::default()
    };
    let before = step.previous_version();
    let asking = proposal(0, &Reads::default(), &Writes::point(b"k"))?.asking_operations();
    step.step_with(
        Input::Commit(CommitRequest::new(1, asking)),
        0,
        &mut out,
        &mut operator,
    );
    step.step_with(Input::Synced, 0, &mut out, &mut operator);
    let outcome = out
        .replies()
        .first()
        .map(|reply| reply.outcome().clone())
        .context("a reply")?;
    Ok((
        outcome,
        operator.discarded,
        step.previous_version() != before,
    ))
}

/// The reply bound falls at its last byte: the largest node whose frame fits is admitted, a byte more is refused naming
/// the frame and the floor, with the operator's work let go. The wire's builder measures each frame.
#[test]
fn test_a_reply_at_the_floor_commits_and_a_byte_past_it_is_refused() -> TestResult {
    let fits = |len: usize| reply_frame(&"n".repeat(len)).is_some_and(|f| f <= MAX_FRAME_FLOOR);
    let (mut low, mut high) = (0, MAX_FRAME_FLOOR);
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if fits(mid) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let (at_floor, discarded_at_floor, took_at_floor) = decided_with_a_node_of(low)?;
    assert_eq!(at_floor, Ok(version(1, 0)), "a node of {low} bytes fits");
    assert_eq!((discarded_at_floor, took_at_floor), (0, true));

    let past = low + 1;
    let size = reply_frame(&"n".repeat(past)).context("within the largest frame")?;
    assert!(size > MAX_FRAME_FLOOR, "{size}");
    let (outcome, discarded, took) = decided_with_a_node_of(past)?;
    let refused = CommitError::reply_too_large(size, MAX_FRAME_FLOOR);
    assert_eq!(outcome, Err(refused.clone()));
    assert_eq!((discarded, took), (1, false));
    assert_eq!(refused.detail(), MAX_FRAME_FLOOR as u64);
    assert!(!refused.is_retryable());
    Ok(())
}
