/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::{PHASES_QUIET_US, PHASES_WINDOW_US};
use ultragraphdb_common_storage::{CommitMark, CommitPhase, CommitPhases, LogIoTotals};

/// The marks of one commit: taken at `at`, in the step 200 µs later, answered 1,000 µs after it was taken, the log having
/// written once for 30 µs and synced once for 450 µs meanwhile, and the core having waited 100 µs on its inbox.
fn marks(at: u64, log: LogIoTotals) -> (CommitMark, CommitMark, CommitMark) {
    let after = LogIoTotals::new(
        (
            log.write_us() + 30,
            log.writes() + 1,
            log.written_bytes() + 4_096,
        ),
        (log.sync_us() + 450, log.syncs() + 1),
    );
    (
        CommitMark::new(at, log, 500),
        CommitMark::new(at + 200, log, 520),
        CommitMark::new(at + 1_000, after, 600),
    )
}

/// A commit's phases are the differences of its marks.
#[test]
fn test_a_commits_phases_are_its_marks_differences() {
    let mut phases = CommitPhases::new();
    let (taken, stepped, answered) = marks(10_000, LogIoTotals::default());
    phases.record(&taken, &stepped, &answered);
    let read = |phase| phases.figures(phase).0;
    assert_eq!(phases.commits(), 1);
    assert_eq!(read(CommitPhase::Total), 1_000);
    assert_eq!(read(CommitPhase::BeforeStep), 200);
    assert_eq!(read(CommitPhase::InStep), 800);
    assert_eq!(read(CommitPhase::LogWrite), 30);
    assert_eq!(read(CommitPhase::LogSync), 450);
    assert_eq!(read(CommitPhase::Waited), 100);
    let total = phases
        .histogram(CommitPhase::Total)
        .map(|h| (h.count(), h.max()));
    assert_eq!(total, Some((1, 1_000)));
}

/// A window runs from the first commit taken to the last answered, and the log's I/O over it is counted once whatever the
/// commits overlapping it.
#[test]
fn test_a_window_spans_its_commits() {
    let mut phases = CommitPhases::new();
    let (taken, stepped, answered) = marks(10_000, LogIoTotals::default());
    phases.record(&taken, &stepped, &answered);
    let (later, later_stepped, later_answered) = marks(10_500, *stepped.log());
    phases.record(&later, &later_stepped, &later_answered);
    assert_eq!(phases.commits(), 2);
    assert_eq!(phases.window_us(), 1_500);
    assert_eq!(phases.log(), LogIoTotals::new((30, 1, 4_096), (450, 1)));
}

/// A window is due a quiet second after its last commit, or a minute after its first, and never while it holds nothing.
#[test]
fn test_a_window_is_due_after_a_pause_or_a_minute() {
    let mut phases = CommitPhases::new();
    assert!(!phases.is_due(u64::MAX), "nothing counted");
    let (taken, stepped, answered) = marks(10_000, LogIoTotals::default());
    phases.record(&taken, &stepped, &answered);
    assert!(!phases.is_due(11_000 + PHASES_QUIET_US - 1));
    assert!(phases.is_due(11_000 + PHASES_QUIET_US));
    for step in 1..=(PHASES_WINDOW_US / 500_000) {
        let (next, next_stepped, next_answered) =
            marks(10_000 + step * 500_000, LogIoTotals::default());
        phases.record(&next, &next_stepped, &next_answered);
    }
    assert!(
        phases.is_due(10_000 + PHASES_WINDOW_US),
        "a minute of commits without a pause"
    );
}

/// A cleared window counts from nothing.
#[test]
fn test_a_cleared_window_counts_again() {
    let mut phases = CommitPhases::new();
    let (taken, stepped, answered) = marks(10_000, LogIoTotals::default());
    phases.record(&taken, &stepped, &answered);
    phases.clear();
    assert_eq!(phases, CommitPhases::default());
    assert_eq!(phases.window_us(), 0);
    assert_eq!(phases.log(), LogIoTotals::default());
}
