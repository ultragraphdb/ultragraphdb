/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a read-write transaction's limit does, the Welcome's `transaction_ms`, which no setting moves: 10 s. Every scenario of
//! it would wait out those 10 s by itself, so one run of them shares one wait: the first scenario to ask runs their
//! conversations together, and each scenario's step takes its outcome.

use std::cell::RefCell;
use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_client_compio::ClientError;
use ultragraphdb_protocol_wire::{DEFAULT_TRANSACTION_MS, ErrorCode};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{
    Answer, Pattern, TENANT, batch_of, read_whole, refusal_of, string, write_of,
};
use crate::cluster::{Carriers, client, shared_client};
use crate::read_steps::{big_pattern, holding_big};
use crate::request_steps::latest_read;
use crate::shells::{ClientAt, TestClient, pause, run};
use crate::transaction_steps::{committed, nothing};

/// Past the limit by this much, what a loaded machine's timers may take.
const PAST: Duration = Duration::from_millis(1_500);

/// What the limit's conversations saw.
#[derive(Debug, Default)]
struct Outcomes {
    unbound: Option<String>,
    bound: Option<(String, Answer)>,
    holding: Option<(String, String)>,
    under_way: Option<String>,
    vanished: Option<(Answer, String)>,
}

thread_local! {
    /// The outcomes of the conversations, once they have run.
    static OUTCOMES: RefCell<Option<Outcomes>> = const { RefCell::new(None) };
}

/// The IRI of `name` in the limit's conversations.
fn iri_of(name: &str) -> String {
    format!("http://ex.org/limit/{name}")
}

/// What code and detail a refusal says, as text a scenario compares.
fn said(error: &ClientError) -> String {
    match refusal_of(error) {
        Ok((code, detail, message)) => format!("{code:?} {detail} {message}"),
        Err(other) => other.to_string(),
    }
}

/// How long a limit's conversation waits.
fn the_limit() -> Duration {
    Duration::from_millis(u64::from(DEFAULT_TRANSACTION_MS)) + PAST
}

async fn unbound(at: &ClientAt) -> TestResult<String> {
    let mut client = client(at).await?;
    let own = client.begin(TENANT, false).await?;
    pause(the_limit()).await;
    let refused = client
        .commit(own, None)
        .await
        .err()
        .context("past its limit")?;
    Ok(said(&refused))
}

async fn bound(at: &ClientAt) -> TestResult<(String, Answer)> {
    let mut client = client(at).await?;
    let (a, p) = (iri_of("bound"), iri_of("p"));
    let own = client.begin(TENANT, false).await?;
    client.write(&own, &write_of(&a, &p, &string("x"))?).await?;
    let _bound = read_whole(&mut client, &own, &Pattern::of(Some(&a), Some(&p))).await?;
    pause(the_limit()).await;
    let refused = client
        .commit(own, None)
        .await
        .err()
        .context("past its limit")?;
    let held = latest_read(&mut client, &Pattern::of(Some(&a), Some(&p))).await?;
    Ok((said(&refused), held))
}

async fn holding(at: &ClientAt) -> TestResult<(String, String)> {
    let mut client = client(at).await?;
    let (a, p) = (iri_of("holding"), iri_of("p"));
    let own = client.begin(TENANT, false).await?;
    let first = client
        .read(&own, &big_pattern().read(&own, 0, Vec::new())?)
        .await?;
    client.write(&own, &write_of(&a, &p, &string("x"))?).await?;
    let pattern = Pattern::of(Some(&a), Some(&p));
    let second = client
        .read(&own, &pattern.read(&own, 0, Vec::new())?)
        .await?;
    let refused = client
        .next_batch(&second, batch_of)
        .await
        .err()
        .context("the second read refused")?;
    Ok((said(&refused), first.stream().to_string()))
}

async fn under_way(at: &ClientAt) -> TestResult<String> {
    let mut client = client(at).await?;
    let own = client.begin(TENANT, false).await?;
    let stream = client
        .read(&own, &big_pattern().read(&own, 0, Vec::new())?)
        .await?;
    let _first = client
        .next_batch(&stream, batch_of)
        .await?
        .context("a batch")?;
    pause(the_limit()).await;
    let mut last = None;
    for _ in 0..64 {
        match client.next_batch(&stream, batch_of).await {
            Ok(Some(_)) => {}
            Ok(None) => return Err(TestError::new("the read reached its LAST past the limit")),
            Err(error) => {
                last = Some(error);
                break;
            }
        }
    }
    Ok(said(&last.context("the read refused")?))
}

async fn vanished(at: &ClientAt) -> TestResult<(Answer, String)> {
    let mut client = client(at).await?;
    let (new, p) = (iri_of("new"), iri_of("p"));
    let own = client.begin(TENANT, false).await?;
    client
        .write(&own, &write_of(&new, &p, &string("x"))?)
        .await?;
    let _bound = read_whole(&mut client, &own, &Pattern::of(Some(&new), Some(&p))).await?;
    pause(the_limit()).await;
    let mut other = crate::cluster::client(at).await?;
    let held = latest_read(&mut other, &Pattern::of(Some(&new), Some(&p))).await?;
    committed(&mut other, &new, &p, &string("later")).await?;
    let again = latest_read(&mut other, &Pattern::of(Some(&new), Some(&p))).await?;
    Ok((held, again.column(0).concat()))
}

/// Runs the limit's conversations together on one runtime.
async fn conversations(at: &ClientAt) -> TestResult<Outcomes> {
    let mut fixture: TestClient = client(at).await?;
    holding_big(&mut fixture).await?;
    let (unbound, bound, holding, under_way, vanished) = futures::join!(
        unbound(at),
        bound(at),
        holding(at),
        under_way(at),
        vanished(at)
    );
    Ok(Outcomes {
        unbound: Some(unbound?),
        bound: Some(bound?),
        holding: Some(holding?),
        under_way: Some(under_way?),
        vanished: Some(vanished?),
    })
}

/// `what` of the conversations' outcomes, run against the shared cluster of `carrier` when no scenario has asked yet.
fn taken<T>(carrier: Carriers, what: impl FnOnce(&mut Outcomes) -> Option<T>) -> TestResult<T> {
    if OUTCOMES.with_borrow(Option::is_none) {
        let at = shared_client(carrier)?;
        let outcomes = run(conversations(&at))?;
        OUTCOMES.set(Some(outcomes));
    }
    OUTCOMES.with_borrow_mut(|outcomes| {
        outcomes
            .as_mut()
            .and_then(what)
            .context("this conversation's outcome")
    })
}

/// That `said` is code 4, `detail` the limit.
fn past_limit(said: &str) {
    let expected = format!("{:?} {DEFAULT_TRANSACTION_MS}", ErrorCode::PastLimit);
    assert!(said.starts_with(&expected), "{said}");
}

#[when("a read-write transaction begins and sends nothing for `transaction_ms`")]
fn unbound_sits(w: &mut Steps) -> TestResult {
    w.seen.facts = vec![taken(w.carrier, |outcomes| outcomes.unbound.take())?];
    Ok(())
}

#[then("the frontend answers code 4 on its stream, with no partition having seen it")]
fn unbound_answered(w: &mut Steps) -> TestResult {
    past_limit(w.seen.facts.first().context("the refusal")?);
    Ok(())
}

#[when(
    "a read-write transaction bound to partition 1 sends nothing for `transaction_ms` from its Begin"
)]
fn bound_sits(w: &mut Steps) -> TestResult {
    let (said, held) = taken(w.carrier, |outcomes| outcomes.bound.take())?;
    w.seen.facts = vec![said];
    w.seen.answers = vec![held];
    Ok(())
}

#[then(
    "the frontend cancels the Bind stream, which aborts it at the partition, and answers code 4 on its stream"
)]
fn bound_answered(w: &mut Steps) -> TestResult {
    past_limit(w.seen.facts.first().context("the refusal")?);
    nothing(w.seen.answers.first().context("what is held")?)
}

#[when(
    "a read of a transaction has spent its credit, the client sends a Write and a second read, waits on the second read, and grants no credit for `transaction_ms`"
)]
fn holding_sits(w: &mut Steps) -> TestResult {
    let (said, stream) = taken(w.carrier, |outcomes| outcomes.holding.take())?;
    w.seen.facts = vec![said, stream];
    Ok(())
}

#[then(
    "the frontend answers code 4, the message naming the first read's stream and that it had no credit"
)]
fn holding_answered(w: &mut Steps) -> TestResult {
    let [said, stream] = w.seen.facts.as_slice() else {
        return Err(TestError::new("the refusal and the stream"));
    };
    past_limit(said);
    assert!(said.contains(stream.as_str()), "{said}");
    assert!(said.contains("credit"), "{said}");
    Ok(())
}

#[when(
    "a read of a transaction is under way, the transaction passes its limit, and compute's Error 1 of `detail` 1 reaches the frontend before the Bind stream's code 4"
)]
fn under_way_sits(w: &mut Steps) -> TestResult {
    w.seen.facts = vec![taken(w.carrier, |outcomes| outcomes.under_way.take())?];
    Ok(())
}

#[then(
    "the frontend holds that Error until the Bind stream ends, and the client's read is refused with code 4"
)]
fn under_way_answered(w: &mut Steps) -> TestResult {
    past_limit(w.seen.facts.first().context("the refusal")?);
    Ok(())
}

#[when(
    "a read-write transaction writes `<new> <p> \"x\"`, creating `<new>`, and its client then sends nothing, its connection staying open"
)]
fn vanished_sits(w: &mut Steps) -> TestResult {
    let (held, later) = taken(w.carrier, |outcomes| outcomes.vanished.take())?;
    w.seen.answers = vec![held];
    w.seen.facts = vec![later];
    Ok(())
}

#[then(
    "the host aborts the transaction once 10 s have passed, nothing it wrote is held, the dictionary holds no entry it created for `<new>`, and a later transaction writing `<new>` commits"
)]
fn vanished_answered(w: &mut Steps) -> TestResult {
    nothing(w.seen.answers.first().context("what is held")?)?;
    assert_eq!(w.seen.facts, ["later"], "{}", w.over());
    Ok(())
}
