/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

use super::LogCapture;
use crate::types::captured_event::CapturedEvent;
use crate::utils::lock::lock;

impl<S: Subscriber> Layer<S> for LogCapture {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let mut captured = CapturedEvent::new(*metadata.level(), metadata.target());
        event.record(&mut captured);
        lock(&self.events).push(captured);
    }
}
