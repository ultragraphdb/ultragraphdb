/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod captured_event_display;
mod captured_event_visit;

use tracing::Level;

/// One `tracing` event as [`LogCapture`](crate::LogCapture) records it: its level, its target and its fields in
/// the order the event names them, the message among them as `message`. It holds no timestamp, so the events of a
/// seeded run are the same each time the seed is run.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapturedEvent {
    level: Level,
    target: String,
    fields: Vec<(String, String)>,
}

impl CapturedEvent {
    /// An event of `level` and `target` with no fields yet.
    pub fn new(level: Level, target: impl Into<String>) -> Self {
        Self {
            level,
            target: target.into(),
            fields: Vec::new(),
        }
    }

    pub fn level(&self) -> Level {
        self.level
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    /// Every field as its name and value, in order: a string as it is, any other value as its `Debug` text.
    pub fn fields(&self) -> &[(String, String)] {
        &self.fields
    }

    /// The value of the field `name`.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The event's message.
    pub fn message(&self) -> Option<&str> {
        self.field("message")
    }
}
