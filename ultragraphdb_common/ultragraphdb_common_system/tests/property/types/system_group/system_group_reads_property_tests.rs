/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Defaults, Challenge and Verify over drawn inputs: laws L3.30 to L3.32 of `notes/laws.md`.

use std::collections::BTreeSet;

use compio::runtime::{Runtime, spawn};
use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_system::{SystemHandle, system_run};
use ultragraphdb_protocol_scram::{ScramClient, ScramRelay};
use ultragraphdb_transactions_log::SimStorage;

use crate::utils::{Clock, SimGroup, recovered, start_of};

/// The characters of a drawn predicate's or name's text.
const LETTERS: &str = "abcdefghijklmnopqrstuvwxyz0123456789";

/// Runs `body` over a fresh group holding `alice` and `bob`, each with the password `pw` and the salt `a salt of <name>`.
fn with_group<T>(
    body: impl AsyncFnOnce(&SystemHandle, &SimStorage, &Clock) -> Option<T>,
) -> Option<T> {
    let runtime = Runtime::new().ok()?;
    runtime.block_on(async {
        let (storage, clock) = (SimStorage::new(5), Clock::new());
        let group: SimGroup = recovered(&storage, &start_of(8, 50)?, &clock).await?;
        let (handle, run) = system_run(group, clock.reading());
        let run = spawn(run);
        let answer = body(&handle, &storage, &clock).await;
        drop(handle);
        run.await.ok()?.ok()?;
        answer
    })
}

/// L3.30: a set of 0 to 64 predicates recorded in a drawn order is answered by Defaults as exactly that set, in the order of
/// their texts' bytes, at a position of at least 1.
#[hegel::test(test_cases = 30)]
fn test_defaults_answer_the_records_in_their_texts_order(tc: TestCase) {
    let count = tc.draw(gs::integers::<usize>().min_value(0).max_value(64));
    let drawn: Vec<String> = (0..count)
        .map(|_| {
            let text: String = tc.draw(gs::text().alphabet(LETTERS).min_size(1).max_size(24));
            format!("http://ex.org/{text}")
        })
        .collect();
    let expected: Vec<String> = drawn
        .iter()
        .cloned()
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();
    let answered = with_group(async |handle, _, _| {
        for predicate in &drawn {
            let _found = handle.record_default(0, predicate.clone()).await.ok()?;
        }
        let defaults = handle.defaults(0).await.ok()?;
        Some((defaults.predicates().to_vec(), defaults.position()))
    });
    let (listed, position) = answered.unwrap_or_default();
    assert_eq!(listed, expected);
    assert!(position >= 1, "{position}");
}

/// L3.31: a held name's Challenge answers its salt and count; a name no principal holds answers one salt each time, before and
/// after a restart, and no two such names share a salt.
#[hegel::test(test_cases = 20)]
fn test_a_challenge_answers_one_salt_a_name(tc: TestCase) {
    let first: String = tc.draw(gs::text().alphabet(LETTERS).min_size(1).max_size(16));
    let second: String = tc.draw(gs::text().alphabet(LETTERS).min_size(1).max_size(16));
    tc.assume(
        first != second
            && !["alice", "bob"].contains(&first.as_str())
            && !["alice", "bob"].contains(&second.as_str()),
    );
    let answered = challenged_across_a_restart(&first, &second);
    assert!(answered.is_some(), "answered");
    if let Some((held, (one, other, same), after)) = answered {
        assert_eq!(held, (4_096, b"a salt of alice".to_vec()));
        assert_eq!(one, same, "one name, one salt");
        assert_eq!(one, after, "the same after a restart");
        assert_ne!(one.1, other.1, "two names, two salts");
        // The count `ultragraphdb_protocol_scram::DEFAULT_ITERATIONS` names.
        assert_eq!(one.0, 600_000);
    }
}

/// L3.32: Verify answers `alice`'s principal and a signature exactly when the proof is the one her password gives over the
/// exchange's AuthMessage; a proof of another password, a proof altered at a drawn byte and a name no principal holds are answered
/// alike, with nothing.
#[hegel::test(test_cases = 20)]
fn test_a_verify_holds_exactly_for_the_right_proof(tc: TestCase) {
    let right = tc.draw(gs::booleans());
    let password = if right {
        "pw".to_string()
    } else {
        tc.draw(gs::text().alphabet(LETTERS).min_size(3).max_size(12))
    };
    let altered = tc.draw(gs::sampled_from(vec![
        None,
        Some(0_usize),
        Some(17),
        Some(31),
    ]));
    let unknown = tc.draw(gs::booleans());
    let name = if unknown { "nobody" } else { "alice" };
    let holds = right && password == "pw" && altered.is_none() && !unknown;
    let answered = with_group(async |handle, _, _| {
        let alice = handle.verifier("alice".to_string()).await?.0;
        let mut client = ScramClient::new(name, &password, "client-nonce").ok()?;
        let mut relay = ScramRelay::new("group-nonce");
        let asked = relay.first(&client.first_message().ok()?).ok()?;
        let (iterations, salt) = handle.challenge(asked).await.ok()?;
        let server_first = relay.challenge(&salt, iterations).ok()?;
        let (auth_message, mut proof) = relay
            .last(&client.final_message(&server_first).ok()?)
            .ok()?;
        if let Some(byte) = altered.and_then(|at| proof.get_mut(at)) {
            *byte ^= 0x01;
        }
        let verified = handle
            .verify(name.to_string(), auth_message.clone(), proof)
            .await
            .ok()?;
        let accepted = verified.map(|(_, signature)| {
            relay
                .finish(&signature)
                .ok()
                .and_then(|last| client.verify(&last).ok())
                .is_some()
        });
        Some((verified.map(|(principal, _)| principal), accepted, alice))
    });
    assert!(answered.is_some(), "answered");
    if let Some((principal, accepted, alice)) = answered {
        if holds {
            assert_eq!((principal, accepted), (Some(alice), Some(true)));
        } else {
            assert_eq!((principal, accepted), (None, None));
        }
    }
}

/// What a Challenge answers: the iteration count and the salt.
type Challenged = (u32, Vec<u8>);

/// Challenges of `alice`, of `first`, `second` and `first` again, and after a restart over the same logs of `first`.
fn challenged_across_a_restart(
    first: &str,
    second: &str,
) -> Option<(Challenged, (Challenged, Challenged, Challenged), Challenged)> {
    let runtime = Runtime::new().ok()?;
    runtime.block_on(async {
        let (storage, clock) = (SimStorage::new(5), Clock::new());
        let start = start_of(8, 50)?;
        let group: SimGroup = recovered(&storage, &start, &clock).await?;
        let (handle, run) = system_run(group, clock.reading());
        let run = spawn(run);
        let held = handle.challenge("alice".to_string()).await.ok()?;
        let before = (
            handle.challenge(first.to_string()).await.ok()?,
            handle.challenge(second.to_string()).await.ok()?,
            handle.challenge(first.to_string()).await.ok()?,
        );
        drop(handle);
        run.await.ok()?.ok()?;
        let restarted: SimGroup = recovered(&storage, &start, &clock).await?;
        let (handle, run) = system_run(restarted, clock.reading());
        let run = spawn(run);
        let after = handle.challenge(first.to_string()).await.ok()?;
        drop(handle);
        run.await.ok()?.ok()?;
        Some((held, before, after))
    })
}
