/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! An operator whose one operation names a node by the IRI it is given, so a test sets the size of the Commit reply a
//! transaction leaves, and the wire's own builder measuring that reply: the independent measure the step's bound is
//! tested against.

use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Commit, Header, OperationResult, WriteSetRecord,
};
use ultragraphdb_transactions_commit::{CommitError, Operator};

use crate::utils::proposal_double::TestProposal;

/// The operator: `iri` is the node its operation names; `discarded` and `admitted` count what the step did with what it
/// left.
#[derive(Debug, Default)]
pub struct NodeOperator {
    /// The IRI the operation's result names.
    pub iri: String,
    /// The staged records the step let go of.
    pub discarded: usize,
    /// The proposals the step admitted.
    pub admitted: usize,
}

impl Operator<TestProposal> for NodeOperator {
    type Staged = Vec<u8>;

    fn begin_round(&mut self) {}

    fn operate<'p, I>(
        &mut self,
        _proposal: &TestProposal,
        _unapplied: I,
        results: &mut Vec<OperationResult>,
    ) -> Result<Vec<u8>, CommitError>
    where
        I: Iterator<Item = &'p TestProposal>,
    {
        results.push(OperationResult::for_node(0, true));
        let mut record = WriteSetRecord::transaction(0);
        record.push_node_result(&self.iri, true);
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

    fn adopt(&mut self, proposal: &mut TestProposal, staged: Vec<u8>) {
        proposal.record = staged;
    }

    fn discard(&mut self, _proposal: &TestProposal, _staged: Vec<u8>) {
        self.discarded += 1;
    }

    fn admitted(&mut self, _proposal: &TestProposal) {
        self.admitted += 1;
    }
}

/// The bytes of the frame of a Commit reply whose one result names `iri`, `None` past the largest frame.
pub fn reply_frame(iri: &str) -> Option<usize> {
    let mut reply = Commit::new([0; 16], 0, 0);
    reply.push_node_result(iri, true);
    reply
        .frame(Header::new(0, Command::Commit))
        .ok()
        .map(|frame| frame.len())
}
