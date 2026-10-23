/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use tracing::Level;
use ultragraphdb_store_sim::{CapturedEvent, LogCapture};

fn one(f: impl FnOnce()) -> CapturedEvent {
    let capture = LogCapture::new();
    tracing::subscriber::with_default(capture.subscriber(), f);
    let mut events = capture.events();
    assert_eq!(events.len(), 1, "{events:?}");
    events.remove(0)
}

#[test]
fn test_new_has_no_fields() {
    let event = CapturedEvent::new(Level::INFO, "t");
    assert_eq!((event.level(), event.target()), (Level::INFO, "t"));
    assert!(event.fields().is_empty());
    assert_eq!(event.message(), None);
    assert_eq!(event.to_string(), "INFO t: ");
}

#[test]
fn test_fields_keep_their_order_strings_as_they_are_and_others_as_debug() {
    let key = vec![0x01u8, 0x02];
    let event = one(|| {
        tracing::error!(target: "ultragraphdb_core_schema", key = ?key, attr = "0-name", ts = 7_u64, ok = true, "Error while parsing key");
    });
    assert_eq!(event.level(), Level::ERROR);
    assert_eq!(event.target(), "ultragraphdb_core_schema");
    let names: Vec<&str> = event.fields().iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["message", "key", "attr", "ts", "ok"]);
    assert_eq!(event.message(), Some("Error while parsing key"));
    assert_eq!(event.field("key"), Some("[1, 2]"));
    assert_eq!(event.field("attr"), Some("0-name"));
    assert_eq!(event.field("ts"), Some("7"));
    assert_eq!(event.field("ok"), Some("true"));
    assert_eq!(event.field("absent"), None);
}

#[test]
fn test_display_puts_the_message_first_and_the_fields_after_it() {
    let event =
        one(|| tracing::info!(target: "t", attr = "0-name", "Deleting schema for predicate"));
    assert_eq!(
        event.to_string(),
        "INFO t: Deleting schema for predicate attr=0-name"
    );
}
