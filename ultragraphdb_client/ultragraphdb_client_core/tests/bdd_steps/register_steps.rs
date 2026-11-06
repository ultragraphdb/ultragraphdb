/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of "The Rust client retries a busy Register".

use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_client_core::{ClientConfig, ClientError, ClientErrorEnum, Reply, TestServer};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{Anonymous, NetworkFaults, TransportError};
use ultragraphdb_protocol_wire::{Command, ErrorCode, FrameView, MAX_FRAME_SIZE};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

use crate::utils::harness::{CLUSTER, Harness, config, server, welcome};
use crate::{Steps, begin, read};

/// A client of `attempts` connected to a server answering its first `busy` Registers code 14.
fn registering(attempts: u32, busy: u32) -> TestResult<Harness> {
    let mut scripted = server();
    scripted.answer_busy_registers(busy);
    let settings: ClientConfig = config().with_commit_attempts(attempts);
    let mut h = Harness::with(settings, scripted, NetworkFaults::none(), 1, Anonymous);
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    Ok(h)
}

/// The streams the server took a Register on, in order.
fn register_streams(h: &Harness) -> TestResult<Vec<u64>> {
    h.server
        .received()
        .iter()
        .filter_map(|frame| FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE).ok())
        .filter(|view| view.command() == Command::Register)
        .map(|view| Ok(view.header().stream()))
        .collect()
}

#[when(
    "a client of 4 commit attempts registers, and a full registry answers its first two Registers code 14"
)]
fn busy_twice(w: &mut Steps) -> TestResult {
    let mut h = registering(4, 2)?;
    let started = h.now;
    let pending = h.core.register(h.now)?;
    let reply = h.settle(pending)?;
    let Ok(Reply::Registered(session)) = reply else {
        return Err(TestError::new(format!("the session: {reply:?}")));
    };
    w.session = Some(session);
    w.waited = h.now.saturating_duration_since(started);
    w.harness = Some(h);
    Ok(())
}

#[then("the client sends the Register three times, each on a new stream, and returns the session")]
fn sent_three_times(w: &mut Steps) -> TestResult {
    let h = w.harness.as_ref().context("a harness")?;
    let streams = register_streams(h)?;
    let distinct: std::collections::BTreeSet<u64> = streams.iter().copied().collect();
    assert_eq!(streams.len(), 3, "the Register's sends: {streams:?}");
    assert_eq!(distinct.len(), streams.len(), "each on a stream of its own");
    assert!(w.session.is_some(), "the session");
    let delays = u32::try_from(streams.len() - 1)?;
    let least: Duration = config().retry_delay() * delays;
    assert!(w.waited >= least, "{:?} is below {least:?}", w.waited);
    Ok(())
}

#[when(
    "a client of 3 commit attempts registers, and a full registry answers every Register code 14"
)]
fn busy_always(w: &mut Steps) -> TestResult {
    let mut h = registering(3, u32::MAX)?;
    let pending = h.core.register(h.now)?;
    let reply = h.settle(pending)?;
    w.errors.push(reply.err().context("a refusal")?);
    w.harness = Some(h);
    Ok(())
}

#[then(
    "the client sends the Register three times and returns code 14, and the connection stays open"
)]
fn busy_returned(w: &mut Steps) -> TestResult {
    let h = w.harness.as_ref().context("a harness")?;
    assert_eq!(register_streams(h)?.len(), 3, "its attempts");
    let error = w.errors.first().context("the refusal")?;
    assert!(
        matches!(
            error.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::SessionBusy,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(h.core.is_ready(), "the connection stays open");
    Ok(())
}

#[when("a Register answered code 14 waits for its next send, and the connection is lost")]
fn lost_while_waiting(w: &mut Steps) -> TestResult {
    let mut h = registering(4, 1)?;
    let pending = h.core.register(h.now)?;
    while h.step()? {}
    assert_eq!(register_streams(&h)?.len(), 1, "its first send");
    assert!(
        h.core.take_reply(pending).is_none(),
        "it waits to be sent again"
    );
    h.core.disconnected(TransportError::closed(), h.now);
    w.errors.push(
        h.core
            .take_reply(pending)
            .context("the answer of the lost connection")?
            .err()
            .context("a refusal")?,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    h.now = h
        .now
        .checked_add(config().retry_delay())
        .context("a later time")?;
    h.core.handle_timeout(h.now);
    while h.step()? {}
    w.harness = Some(h);
    Ok(())
}

#[then("the client returns the lost connection, and sends the Register on no later connection")]
fn lost_returned(w: &mut Steps) -> TestResult {
    let h = w.harness.as_ref().context("a harness")?;
    let error = w.errors.first().context("the answer")?;
    assert!(
        matches!(error.0, ClientErrorEnum::ConnectionLost { .. }),
        "{error:?}"
    );
    assert_eq!(register_streams(h)?.len(), 1, "no later send");
    Ok(())
}

#[when("a server sends a read's stream a Result frame with the stream's credit spent")]
fn past_credit(w: &mut Steps) -> TestResult {
    let mut scripted = TestServer::new(CLUSTER, welcome().with_initial_credit(1));
    scripted.send_past_credit();
    let mut h = Harness::ready(scripted)?;
    let tx = begin(&mut h, true)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    h.until(|h| !h.core.is_connected())?;
    w.numbers.push(stream.stream());
    w.stream = Some(stream);
    w.harness = Some(h);
    Ok(())
}

#[then("the client refuses it as `PastCredit`, naming the stream, and closes the connection")]
fn past_credit_refused(w: &mut Steps) -> TestResult {
    let named = *w.numbers.first().context("the read's stream")?;
    let stream = w.stream.as_ref().context("the read")?;
    let h = w.harness.as_mut().context("a harness")?;
    assert!(!h.core.is_connected(), "the connection is closed");
    let error = h
        .core
        .next_batch(stream, h.now, |_| ())
        .err()
        .context("the read's answer")?;
    let ClientErrorEnum::ConnectionLost { cause: Some(cause) } = &error.0 else {
        return Err(TestError::new(format!(
            "the refusal as the cause: {error:?}"
        )));
    };
    assert_eq!(
        cause.0,
        ClientError::protocol(ProtocolError::past_credit(named)).0
    );
    assert!(
        error.to_string().contains(&format!("stream {named}")),
        "it names the stream: {error}"
    );
    Ok(())
}
