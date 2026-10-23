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
    tracing::subscriber::with_default(clone.subscriber(), || tracing::info!("seen"));
    assert_eq!(capture.events().len(), 1);
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
    assert!(first.len() >= 60, "{first:?}");
    assert_eq!(first, run_events(11));
    assert_ne!(first, run_events(12), "another seed picks other actors");
}
