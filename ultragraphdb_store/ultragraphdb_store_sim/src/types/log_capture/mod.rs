/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod log_capture_layer;

use std::sync::{Arc, Mutex};

use tracing::Subscriber;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

use crate::types::captured_event::CapturedEvent;
use crate::utils::lock::lock;

/// Records the `tracing` events of a test, in the order they are emitted, at every level, without timestamps: a
/// crate's simulator test installs [`subscriber`](Self::subscriber) on its own thread with
/// `tracing::subscriber::with_default`, runs its workload, and reads [`events`](Self::events). A clone records
/// into the same list.
///
/// ```
/// use ultragraphdb_store_sim::LogCapture;
///
/// let capture = LogCapture::new();
/// tracing::subscriber::with_default(capture.subscriber(), || {
///     tracing::warn!(target: "ultragraphdb_core_tok", lang = "xx", "Unable to find lang. Reverting to English.");
/// });
/// let events = capture.events();
/// assert_eq!(events[0].field("lang"), Some("xx"));
/// assert_eq!(
///     events[0].to_string(),
///     "WARN ultragraphdb_core_tok: Unable to find lang. Reverting to English. lang=xx"
/// );
/// ```
#[derive(Debug, Clone, Default)]
pub struct LogCapture {
    events: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl LogCapture {
    pub fn new() -> Self {
        Self::default()
    }

    /// A subscriber recording into this capture.
    pub fn subscriber(&self) -> impl Subscriber + Send + Sync + 'static {
        Registry::default().with(self.clone())
    }

    /// The events recorded so far, oldest first.
    pub fn events(&self) -> Vec<CapturedEvent> {
        lock(&self.events).clone()
    }
}
