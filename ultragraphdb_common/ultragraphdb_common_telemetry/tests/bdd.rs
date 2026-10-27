/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;

use cucumber::{World, then, when};
use opentelemetry_sdk::trace::SpanData;
use serde_json::Value;
use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, subscriber};

#[path = "test_support.rs"]
mod test_support;

use crate::test_support::{Buffer, Kept};

#[derive(Debug, Default, World)]
struct Steps {
    lines: Vec<Value>,
    spans: Vec<SpanData>,
}

#[when(
    "telemetry is installed for JSON output at level `info` and an `info` event with a field `key` is emitted"
)]
fn json_event(w: &mut Steps) {
    let buffer = Buffer::default();
    let writer = buffer.clone();
    let (subscriber, _guard) = subscriber(
        &TelemetryConfig::new("info", LogFormat::Json),
        move || writer.clone(),
        None::<Kept>,
    )
    .expect("the filter parses");
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: "ultragraphdb_core_schema", key = "0-name", "Deleting schema for predicate");
        tracing::debug!(target: "ultragraphdb_core_schema", key = "0-age", "below the level");
    });
    w.lines = buffer
        .text()
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is one JSON object"))
        .collect();
}

#[then(
    "one JSON line holds the level, the target, the message and `key`, and a `debug` event emits nothing"
)]
fn json_line(w: &mut Steps) {
    assert_eq!(w.lines.len(), 1, "{:?}", w.lines);
    let line = &w.lines[0];
    assert_eq!(line["level"], "INFO");
    assert_eq!(line["target"], "ultragraphdb_core_schema");
    assert_eq!(line["fields"]["message"], "Deleting schema for predicate");
    assert_eq!(line["fields"]["key"], "0-name");
}

#[when("telemetry is installed with an exporter and a span is entered inside another")]
fn nested_spans(w: &mut Steps) {
    let exporter = Kept::default();
    let (subscriber, guard) = subscriber(
        &TelemetryConfig::new("info", LogFormat::Plain),
        io::sink,
        Some(exporter.clone()),
    )
    .expect("the filter parses");
    tracing::subscriber::with_default(subscriber, || {
        tracing::info_span!("outer").in_scope(|| tracing::info_span!("inner").in_scope(|| {}));
    });
    drop(guard);
    w.spans = exporter
        .0
        .get_finished_spans()
        .expect("the exporter's lock");
}

#[then("the exporter receives both spans, the inner one naming the outer as its parent")]
fn linked_spans(w: &mut Steps) {
    assert_eq!(w.spans.len(), 2, "{:?}", w.spans);
    let outer = w
        .spans
        .iter()
        .find(|s| s.name == "outer")
        .expect("the outer span");
    let inner = w
        .spans
        .iter()
        .find(|s| s.name == "inner")
        .expect("the inner span");
    assert_eq!(inner.parent_span_id, outer.span_context.span_id());
    assert_eq!(inner.span_context.trace_id(), outer.span_context.trace_id());
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
