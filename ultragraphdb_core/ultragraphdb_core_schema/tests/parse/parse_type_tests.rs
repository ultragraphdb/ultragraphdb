/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The warning Dgraph's `parseTypeField` logs for a field declared in the old format.

use tracing::Level;
use ultragraphdb_core_schema::parse;
use ultragraphdb_store_sim::LogCapture;

#[test]
fn a_field_with_its_old_type_is_parsed_with_a_warning() {
    let capture = LogCapture::new();
    tracing::subscriber::with_default(capture.subscriber(), || {
        parse("type Person {\n name: [string!]!\n age\n}").expect("the old format parses");
    });
    let events = capture.events();
    assert_eq!(
        events.len(),
        1,
        "only the field with a type warns: {events:?}"
    );
    assert_eq!(events[0].level(), Level::WARN);
    assert_eq!(
        events[0].message(),
        Some(
            "Type declaration includes deprecated information about field type which will be ignored."
        )
    );
    assert_eq!(events[0].field("type_name"), Some("0-Person"));
    assert_eq!(events[0].field("field"), Some("name"));
}
