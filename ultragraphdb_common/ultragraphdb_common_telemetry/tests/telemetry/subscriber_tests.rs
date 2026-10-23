/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;

use opentelemetry::trace::{SpanId, TraceId};
use opentelemetry_sdk::trace::SpanData;
use serde_json::Value;
use tracing::{debug, info, info_span, warn};
use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, TelemetryErrorEnum, subscriber};

use crate::test_support::{Buffer, Kept};

/// The events written while `f` runs under the subscriber of `filter` and `format`.
fn logged(filter: &str, format: LogFormat, f: impl FnOnce()) -> String {
    let buffer = Buffer::default();
    let writer = buffer.clone();
    let (subscriber, _guard) = subscriber(
        &TelemetryConfig::new(filter, format),
        move || writer.clone(),
        None::<Kept>,
    )
    .expect("the filter parses");
    tracing::subscriber::with_default(subscriber, f);
    buffer.text()
}

/// The spans exported while `f` runs under the subscriber of `filter`, read after the guard is dropped.
fn exported(filter: &str, f: impl FnOnce()) -> Vec<SpanData> {
    let exporter = Kept::default();
    let (subscriber, guard) = subscriber(
        &TelemetryConfig::new(filter, LogFormat::Plain),
        io::sink,
        Some(exporter.clone()),
    )
    .expect("the filter parses");
    tracing::subscriber::with_default(subscriber, f);
    drop(guard);
    exporter
        .0
        .get_finished_spans()
        .expect("the exporter's lock")
}

#[test]
fn test_plain_lines_carry_level_target_message_and_fields() {
    let text = logged("info", LogFormat::Plain, || {
        info!(target: "ultragraphdb_core_schema", predicate = "0-name", ts = 7, "Deleting schema for predicate");
    });
    let line = text.lines().next().expect("one event");
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(
        line.contains(" INFO ultragraphdb_core_schema: Deleting schema for predicate"),
        "{line}"
    );
    assert!(line.contains("predicate=\"0-name\" ts=7"), "{line}");
    assert!(
        !line.contains('\x1b'),
        "plain output carries no colour codes: {line:?}"
    );
}

#[test]
fn test_json_lines_parse_and_carry_level_target_message_and_fields() {
    let text = logged("info", LogFormat::Json, || {
        info!(target: "ultragraphdb_core_schema", predicate = "0-name", ts = 7, "Deleting schema for predicate");
        warn!(target: "ultragraphdb_core_tok", lang = "xx", "Unable to find lang. Reverting to English.");
    });
    let lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is one JSON object"))
        .collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert_eq!(lines[0]["level"], "INFO");
    assert_eq!(lines[0]["target"], "ultragraphdb_core_schema");
    assert_eq!(
        lines[0]["fields"]["message"],
        "Deleting schema for predicate"
    );
    assert_eq!(lines[0]["fields"]["predicate"], "0-name");
    assert_eq!(lines[0]["fields"]["ts"], 7);
    assert_eq!(lines[1]["level"], "WARN");
    assert_eq!(lines[1]["target"], "ultragraphdb_core_tok");
    assert_eq!(lines[1]["fields"]["lang"], "xx");
}

#[test]
fn test_filter_drops_events_below_its_level() {
    let text = logged("warn", LogFormat::Plain, || {
        debug!("dropped debug");
        info!("dropped info");
        warn!("kept warn");
    });
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("kept warn"), "{text}");
}

#[test]
fn test_filter_directive_sets_a_level_per_target() {
    let text = logged(
        "error,ultragraphdb_store_kv=debug",
        LogFormat::Plain,
        || {
            debug!(target: "ultragraphdb_store_kv", "kept debug");
            debug!(target: "ultragraphdb_store_posting", "dropped debug");
        },
    );
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("kept debug"), "{text}");
}

#[test]
fn test_invalid_filter_is_refused() {
    let result = subscriber(
        &TelemetryConfig::new("ultragraphdb=loud", LogFormat::Plain),
        io::sink,
        None::<Kept>,
    );
    let Err(error) = result else {
        panic!("the directive does not parse");
    };
    assert!(
        matches!(&error.0, TelemetryErrorEnum::Filter { directive, .. } if directive == "ultragraphdb=loud"),
        "{error}"
    );
}

#[test]
fn test_spans_reach_the_exporter_with_parent_and_child_linked() {
    let spans = exported("info", || {
        info_span!("apply").in_scope(|| {
            info_span!("commit", ts = 9).in_scope(|| info!("committed"));
        });
    });
    assert_eq!(spans.len(), 2, "{spans:?}");
    let parent = spans
        .iter()
        .find(|s| s.name == "apply")
        .expect("the parent span");
    let child = spans
        .iter()
        .find(|s| s.name == "commit")
        .expect("the child span");
    assert_eq!(
        parent.parent_span_id,
        SpanId::INVALID,
        "the parent is a root"
    );
    assert_eq!(child.parent_span_id, parent.span_context.span_id());
    assert_eq!(
        child.span_context.trace_id(),
        parent.span_context.trace_id()
    );
    assert_ne!(child.span_context.trace_id(), TraceId::INVALID);
    assert_eq!(
        child.events.events.first().map(|e| e.name.as_ref()),
        Some("committed"),
        "the child carries its event"
    );
}

#[test]
fn test_spans_below_the_filter_are_not_exported() {
    let spans = exported("info", || {
        tracing::debug_span!("hidden").in_scope(|| {});
        info_span!("shown").in_scope(|| {});
    });
    let names: Vec<&str> = spans.iter().map(|s| s.name.as_ref()).collect();
    assert_eq!(names, vec!["shown"]);
}

#[test]
fn test_spans_of_the_exporter_stack_are_not_exported() {
    let spans = exported("trace", || {
        info_span!(target: "h2", "frame").in_scope(|| {});
        info_span!(target: "tonic", "request").in_scope(|| {});
        info_span!(target: "ultragraphdb_store_kv", "batch").in_scope(|| {});
    });
    let names: Vec<&str> = spans.iter().map(|s| s.name.as_ref()).collect();
    assert_eq!(names, vec!["batch"]);
}
