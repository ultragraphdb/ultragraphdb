/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The test server's session table, deciding a request as `PROTOCOL.md` §6.6 has a frontend decide it: a request below
//! the session's last, or a transaction's Commit at its last, is refused with code 13 naming the last, applying nothing.

use ultragraphdb_client_core::{ClientError, ClientErrorEnum, Reply, Session, Transaction};
use ultragraphdb_protocol_wire::{DirectWrite, DropCommand, ErrorCode, TermTable, WriteChange};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::harness::{Harness, begun, committed, registered, server};

fn session(h: &mut Harness) -> TestResult<Session> {
    let p = h.core.register(h.now)?;
    registered(h.settle(p)?)
}

fn begin(h: &mut Harness) -> TestResult<Transaction> {
    let p = h.core.begin(0, false, h.now)?;
    begun(h.settle(p)?)
}

fn insert() -> DirectWrite {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    DirectWrite::new(
        3,
        terms,
        vec![WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: a,
            object: a,
        }],
    )
}

/// The code and detail of a refusal.
fn refusal(answer: Result<Reply, ClientError>) -> Option<(ErrorCode, u64)> {
    if let ClientErrorEnum::Refused { code, detail, .. } = answer.err()?.0 {
        return Some((code, detail));
    }
    None
}

/// A session whose last committed request is 2, its counter set back to `next` as a resumed session's may be.
fn resumed_below(h: &mut Harness, next: u64) -> TestResult<Session> {
    let s = session(h)?;
    for request in 1..=2 {
        let tx = begin(h)?;
        let p = h.core.commit(tx, s, h.now)?;
        assert_eq!(committed(h.settle(p)?)?.request(), request);
    }
    Ok(h.core.resume_session(s.number(), next)?)
}

#[test]
fn test_a_commit_below_or_at_the_sessions_last_is_refused_as_out_of_order() -> TestResult {
    let out_of_order = Some((ErrorCode::RequestOutOfOrder, 2));
    for next in [1, 2] {
        let mut h = Harness::ready(server())?;
        let s = resumed_below(&mut h, next)?;
        let tx = begin(&mut h)?;
        let p = h.core.commit(tx, s, h.now)?;
        assert_eq!(refusal(h.settle(p)?), out_of_order, "request {next}");
        assert_eq!(h.server.applied(), 2, "request {next} applied nothing");
    }
    Ok(())
}

#[test]
fn test_a_one_frame_write_or_drop_below_the_sessions_last_is_refused_as_out_of_order() -> TestResult
{
    let out_of_order = Some((ErrorCode::RequestOutOfOrder, 2));
    let mut h = Harness::ready(server())?;
    let s = resumed_below(&mut h, 1)?;
    let writing = h.core.direct_write(insert(), s, h.now)?;
    assert_eq!(refusal(h.settle(writing)?), out_of_order, "the DirectWrite");
    let again = h.core.resume_session(s.number(), 1)?;
    let dropping = h
        .core
        .drop_command(DropCommand::tenant_data(0), again, h.now)?;
    assert_eq!(refusal(h.settle(dropping)?), out_of_order, "the Drop");
    assert_eq!(h.server.applied(), 2, "neither applied");
    Ok(())
}

/// The transactions the server holds open: a Begin's until its Commit.
#[test]
fn test_the_server_counts_its_open_transactions() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    assert_eq!(h.server.open_transactions(), 1);
    let p = h.core.commit(tx, s, h.now)?;
    let _committed = committed(h.settle(p)?)?;
    assert_eq!(h.server.open_transactions(), 0);
    Ok(())
}
