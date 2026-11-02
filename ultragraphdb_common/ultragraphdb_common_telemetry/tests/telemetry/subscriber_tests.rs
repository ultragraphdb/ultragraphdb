/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;

use std::time::Duration;

use opentelemetry::trace::{SpanId, TraceId};
use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::trace::SpanData;
use serde_json::Value;
use tracing::{debug, error, info, info_span, trace, warn};
use ultragraphdb_common_telemetry::{
    Dial, LogFormat, OtlpHttpClient, TelemetryConfig, TelemetryError, TelemetryErrorEnum,
    subscriber,
};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transport_tls::{TestPki, TlsConnector, TlsTrust};

use crate::collector::{CollectorDial, TlsCollectorDial};
use crate::test_support::{Buffer, Kept};

/// The events written while `f` runs under the subscriber of `filter` and `format`.
fn logged(filter: &str, format: LogFormat, f: impl FnOnce()) -> TestResult<String> {
    let buffer = Buffer::default();
    let writer = buffer.clone();
    let (subscriber, _guard) = subscriber(
        &TelemetryConfig::new(filter, format),
        move || writer.clone(),
        None::<Kept>,
    )
    .context("the filter parses")?;
    tracing::subscriber::with_default(subscriber, f);
    buffer.text()
}

/// The spans exported while `f` runs under the subscriber of `filter`, read after the guard is dropped.
fn exported(filter: &str, f: impl FnOnce()) -> TestResult<Vec<SpanData>> {
    let exporter = Kept::default();
    let (subscriber, guard) = subscriber(
        &TelemetryConfig::new(filter, LogFormat::Plain),
        io::sink,
        Some(exporter.clone()),
    )
    .context("the filter parses")?;
    tracing::subscriber::with_default(subscriber, f);
    drop(guard);
    exporter
        .0
        .get_finished_spans()
        .context("the exporter's lock")
}

#[test]
fn test_plain_lines_carry_level_target_message_and_fields() -> TestResult {
    let text = logged("info", LogFormat::Plain, || {
        info!(target: "ultragraphdb_core_schema", predicate = "0-name", ts = 7, "Deleting schema for predicate");
    })?;
    let line = text.lines().next().context("one event")?;
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
    Ok(())
}

#[test]
fn test_json_lines_parse_and_carry_level_target_message_and_fields() -> TestResult {
    let text = logged("info", LogFormat::Json, || {
        info!(target: "ultragraphdb_core_schema", predicate = "0-name", ts = 7, "Deleting schema for predicate");
        warn!(target: "ultragraphdb_core_tok", lang = "xx", "Unable to find lang. Reverting to English.");
    })?;
    let lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).context("each line is one JSON object"))
        .collect::<TestResult<_>>()?;
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
    Ok(())
}

#[test]
fn test_filter_drops_events_below_its_level() -> TestResult {
    let text = logged("warn", LogFormat::Plain, || {
        debug!("dropped debug");
        info!("dropped info");
        warn!("kept warn");
    })?;
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("kept warn"), "{text}");
    Ok(())
}

#[test]
fn test_filter_directive_sets_a_level_per_target() -> TestResult {
    let text = logged(
        "error,ultragraphdb_store_kv=debug",
        LogFormat::Plain,
        || {
            debug!(target: "ultragraphdb_store_kv", "kept debug");
            debug!(target: "ultragraphdb_store_posting", "dropped debug");
        },
    )?;
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("kept debug"), "{text}");
    Ok(())
}

/// The messages `filter` passes of one event at each level, from `trace` up to `error`.
fn passed(filter: &str) -> TestResult<Vec<String>> {
    let text = logged(filter, LogFormat::Plain, || {
        trace!("at trace");
        debug!("at debug");
        info!("at info");
        warn!("at warn");
        error!("at error");
    })?;
    text.lines()
        .map(|line| {
            let at = line.find("at ").context("a message")?;
            Ok(line[at..].to_string())
        })
        .collect()
}

#[test]
fn test_each_level_passes_itself_and_every_level_above_it() -> TestResult {
    // tracing's order, most verbose first: trace, debug, info, warn, error; `off` passes nothing.
    let levels = ["at trace", "at debug", "at info", "at warn", "at error"];
    for (filter, from) in [
        ("trace", 0),
        ("debug", 1),
        ("info", 2),
        ("warn", 3),
        ("error", 4),
        ("off", 5),
    ] {
        assert_eq!(passed(filter)?, levels[from..].to_vec(), "{filter}");
    }
    Ok(())
}

#[test]
fn test_level_names_ignore_case_and_numbers_name_levels() -> TestResult {
    // EnvFilter reads a level case-insensitively, and the numbers 0-5 as off, error, warn, info, debug, trace.
    assert_eq!(passed("WARN")?, ["at warn", "at error"]);
    assert_eq!(passed("2")?, ["at warn", "at error"]);
    assert_eq!(passed("0")?, Vec::<String>::new());
    Ok(())
}

#[test]
fn test_an_empty_filter_passes_nothing() -> TestResult {
    // An empty directive parses into no directive at all, and EnvFilter then enables nothing, errors included
    // (measured against tracing-subscriber 0.3.23): the filter is the caller's, with no default added.
    assert_eq!(passed("")?, Vec::<String>::new());
    Ok(())
}

#[test]
fn test_invalid_filters_are_refused_with_the_directive_and_the_parsers_reason() -> TestResult {
    // The reasons are tracing-subscriber 0.3.23's, as `EnvFilter::try_new` gives them.
    let level = "error parsing level filter: expected one of \"off\", \"error\", \"warn\", \"info\", \"debug\", \"trace\", or a number 0-5";
    for (directive, reason) in [
        ("ultragraphdb=loud", level),
        ("=info", "invalid filter directive"),
        ("info,=debug", "invalid filter directive"),
        ("[{", "invalid filter directive"),
        (" ", "invalid filter directive"),
    ] {
        let result = subscriber(
            &TelemetryConfig::new(directive, LogFormat::Plain),
            io::sink,
            None::<Kept>,
        );
        let error = result
            .err()
            .ok_or_else(|| format!("{directive:?} does not parse"))
            .context("an invalid directive is refused")?;
        assert_eq!(
            error,
            TelemetryError::Filter(directive.to_string(), reason.to_string())
        );
    }
    Ok(())
}

#[test]
fn test_invalid_filter_is_refused() -> TestResult {
    let result = subscriber(
        &TelemetryConfig::new("ultragraphdb=loud", LogFormat::Plain),
        io::sink,
        None::<Kept>,
    );
    let error = result.err().context("the directive does not parse")?;
    assert!(
        matches!(&error.0, TelemetryErrorEnum::Filter { directive, .. } if directive == "ultragraphdb=loud"),
        "{error}"
    );
    Ok(())
}

#[test]
fn test_spans_reach_the_exporter_with_parent_and_child_linked() -> TestResult {
    let spans = exported("info", || {
        info_span!("apply").in_scope(|| {
            info_span!("commit", ts = 9).in_scope(|| info!("committed"));
        });
    })?;
    assert_eq!(spans.len(), 2, "{spans:?}");
    let parent = spans
        .iter()
        .find(|s| s.name == "apply")
        .context("the parent span")?;
    let child = spans
        .iter()
        .find(|s| s.name == "commit")
        .context("the child span")?;
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
    Ok(())
}

#[test]
fn test_spans_below_the_filter_are_not_exported() -> TestResult {
    let spans = exported("info", || {
        tracing::debug_span!("hidden").in_scope(|| {});
        info_span!("shown").in_scope(|| {});
    })?;
    let names: Vec<&str> = spans.iter().map(|s| s.name.as_ref()).collect();
    assert_eq!(names, vec!["shown"]);
    Ok(())
}

#[test]
fn test_spans_of_the_exporter_stack_are_not_exported() -> TestResult {
    let spans = exported("trace", || {
        info_span!(target: "opentelemetry", "opentelemetry").in_scope(|| {});
        info_span!(target: "opentelemetry_http", "opentelemetry_http").in_scope(|| {});
        info_span!(target: "opentelemetry_otlp", "opentelemetry_otlp").in_scope(|| {});
        info_span!(target: "opentelemetry_sdk", "opentelemetry_sdk").in_scope(|| {});
        info_span!(target: "ultragraphdb_store_kv", "batch").in_scope(|| {});
    })?;
    let names: Vec<&str> = spans.iter().map(|s| s.name.as_ref()).collect();
    assert_eq!(names, vec!["batch"]);
    Ok(())
}

#[test]
fn test_dropping_the_guard_exports_while_the_subscriber_lives_on() -> TestResult {
    // `init` installs the subscriber for good, so its tracer outlives the guard: the guard's drop alone flushes the
    // batch processor, whose scheduled export is seconds away.
    let exporter = Kept::default();
    let (subscriber, guard) = subscriber(
        &TelemetryConfig::new("info", LogFormat::Plain),
        io::sink,
        Some(exporter.clone()),
    )
    .context("the filter parses")?;
    let dispatch = tracing::Dispatch::new(subscriber);
    tracing::dispatcher::with_default(&dispatch, || info_span!("kept").in_scope(|| {}));
    drop(guard);
    let names: Vec<String> = exporter
        .0
        .get_finished_spans()
        .context("the exporter's lock")?
        .iter()
        .map(|s| s.name.to_string())
        .collect();
    assert_eq!(names, ["kept"]);
    drop(dispatch);
    Ok(())
}

#[test]
fn test_spans_reach_a_collector_over_otlp_http_without_a_runtime() -> TestResult {
    let dial = CollectorDial::answering(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
    spans_reach_a_collector(
        "http://collector:4318",
        OtlpHttpClient::over(dial.clone(), Duration::from_secs(5)),
    )?;
    assert_eq!(dial.dialed().len(), 1);
    received_one_span(&dial.received())
}

#[test]
fn test_spans_reach_a_collector_over_otlp_https_verified_against_its_ca() -> TestResult {
    let pki = TestPki::new()?;
    let dial = TlsCollectorDial::answering(&pki, b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
    let trust = TlsTrust::from_pem("test CA", pki.ca_pem().as_bytes()).context("the CA")?;
    let client = OtlpHttpClient::over(dial.clone(), Duration::from_secs(5))
        .with_tls(TlsConnector::new(trust)?);
    spans_reach_a_collector("https://localhost:4318", client)?;
    received_one_span(&dial.plain().received())
}

/// Exports one span through `client` to the collector at `base`, the exporter as init builds it; the guard's drop
/// flushes the batch on the processor's thread.
fn spans_reach_a_collector<D: Dial + 'static>(base: &str, client: OtlpHttpClient<D>) -> TestResult {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(format!("{base}/v1/traces"))
        .with_http_client(client)
        .build()
        .context("an exporter")?;
    let (subscriber, guard) = subscriber(
        &TelemetryConfig::new("info", LogFormat::Plain),
        io::sink,
        Some(exporter),
    )
    .context("the filter parses")?;
    tracing::subscriber::with_default(subscriber, || info_span!("apply_round").in_scope(|| {}));
    drop(guard);
    Ok(())
}

/// Checks that `request` is the exporter's POST of protobuf carrying the span `apply_round`.
fn received_one_span(request: &str) -> TestResult {
    let (head, body) = request.split_once("\r\n\r\n").context("a request head")?;
    assert!(head.starts_with("POST /v1/traces HTTP/1.1\r\n"), "{head}");
    assert!(
        head.contains("content-type: application/x-protobuf"),
        "{head}"
    );
    assert!(
        body.contains("apply_round"),
        "the protobuf body carries the span's name"
    );
    Ok(())
}
