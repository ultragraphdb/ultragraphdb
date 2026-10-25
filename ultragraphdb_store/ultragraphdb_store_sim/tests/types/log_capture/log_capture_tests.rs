/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use tracing::Level;
use ultragraphdb_store_sim::{Event, FaultConfig, LogCapture, Seed, Simulation};

#[test]
fn test_records_every_level_in_order() {
    let capture = LogCapture::new();
    tracing::subscriber::with_default(capture.subscriber(), || {
        tracing::trace!("one");
        tracing::debug!("two");
        tracing::info!("three");
        tracing::warn!("four");
        tracing::error!("five");
    });
    let seen: Vec<(Level, String)> = capture
        .events()
        .iter()
        .map(|e| (e.level(), e.message().unwrap_or_default().to_string()))
        .collect();
    assert_eq!(
        seen,
        vec![
            (Level::TRACE, "one".to_string()),
            (Level::DEBUG, "two".to_string()),
            (Level::INFO, "three".to_string()),
            (Level::WARN, "four".to_string()),
            (Level::ERROR, "five".to_string()),
        ]
    );
}

#[test]
fn test_a_clone_records_into_the_same_list() {
    let capture = LogCapture::new();
    let clone = capture.clone();
    tracing::subscriber::with_default(clone.subscriber(), || tracing::info!("first"));
    tracing::subscriber::with_default(capture.subscriber(), || tracing::info!("second"));
    let messages = |c: &LogCapture| -> Vec<String> {
        c.events()
            .iter()
            .map(|e| e.message().unwrap_or_default().to_string())
            .collect()
    };
    assert_eq!(messages(&capture), vec!["first", "second"]);
    assert_eq!(messages(&clone), messages(&capture));
    assert!(
        LogCapture::new().events().is_empty(),
        "a new capture starts empty"
    );
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the test emits on a second thread to prove the capture records its own thread alone"
)]
fn test_events_of_other_threads_are_not_recorded() {
    let capture = LogCapture::new();
    tracing::subscriber::with_default(capture.subscriber(), || {
        tracing::info!("here");
        std::thread::spawn(|| tracing::info!("there"))
            .join()
            .expect("the other thread ended");
    });
    let messages: Vec<String> = capture
        .events()
        .iter()
        .map(|e| e.message().unwrap_or_default().to_string())
        .collect();
    assert_eq!(messages, vec!["here".to_string()]);
}

/// The events a seeded run emits: one per step, naming the actor the scheduler picked.
fn run_events(seed: u64) -> Vec<String> {
    let capture = LogCapture::new();
    tracing::subscriber::with_default(capture.subscriber(), || {
        let mut simulation = Simulation::new(Seed::new(seed), FaultConfig::mixed());
        let _ = simulation.run(60, 3, |ctx, event| {
            match event {
                Event::Step(actor) => tracing::info!(step = ctx.step(), actor, "stepped"),
                Event::Recovered => tracing::warn!(step = ctx.step(), "recovered"),
            }
            Ok(())
        });
    });
    capture.events().iter().map(ToString::to_string).collect()
}

#[test]
fn test_a_seed_replays_its_events() {
    let first = run_events(11);
    // No task is spawned, so every one of the 60 steps steps an actor, in step order.
    let stepped: Vec<&String> = first.iter().filter(|e| e.contains(": stepped ")).collect();
    assert_eq!(stepped.len(), 60, "{first:?}");
    assert!(
        stepped
            .iter()
            .enumerate()
            .all(|(i, e)| e
                .starts_with(&format!("INFO {}: stepped step={i} actor=", module_path!()))),
        "{stepped:?}"
    );
    let recovered = first.iter().filter(|e| e.contains(": recovered ")).count();
    assert_eq!(stepped.len() + recovered, first.len(), "{first:?}");
    assert!(recovered > 0, "mixed faults recover the store: {first:?}");
    assert_eq!(first, run_events(11));
    assert_ne!(first, run_events(12), "another seed picks other actors");
}

fn emitted_once() {
    tracing::warn!(
        site = "shared",
        "a callsite first reached without a subscriber"
    );
}

/// Tracing caches each callsite's interest process-wide: a callsite first reached on a thread without a subscriber
/// can be cached as uninteresting while another test captures. `capture` rebuilds the cache under its subscriber,
/// so the replay tests of every crate record each event their run emits.
#[test]
fn test_capture_records_a_callsite_first_reached_without_a_subscriber() {
    emitted_once();
    let capture = LogCapture::new();
    let answer = capture.capture(|| {
        emitted_once();
        42
    });
    assert_eq!(answer, 42);
    let events = capture.events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].field("site"), Some("shared"));
}

fn reached_first_elsewhere() {
    tracing::info!(
        site = "elsewhere",
        "a callsite another thread reaches first"
    );
}

/// A callsite another thread reaches first, while the capture's subscriber is the only one alive, is recorded on
/// the capture's thread: tracing would compute its interest from the other thread's default alone and cache it.
#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the test reaches the callsite first on a second thread"
)]
fn test_capture_records_a_callsite_another_thread_reached_first() {
    let capture = LogCapture::new();
    capture.capture(|| {
        std::thread::spawn(reached_first_elsewhere)
            .join()
            .expect("the other thread ended");
        reached_first_elsewhere();
    });
    let events = capture.events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].field("site"), Some("elsewhere"));
}
