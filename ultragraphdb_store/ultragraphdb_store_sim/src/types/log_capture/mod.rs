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
/// crate's simulator test runs its workload inside [`capture`](Self::capture), which installs
/// [`subscriber`](Self::subscriber) on its own thread, and reads [`events`](Self::events). A clone records into the
/// same list.
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

    /// Runs `f` with this capture's subscriber as the thread's default, first rebuilding tracing's callsite interest
    /// cache under it: the cache is process-wide, and a callsite another thread reached first without a subscriber
    /// would otherwise stay cached as uninteresting and lose its events here. What a replay test wraps its run in.
    ///
    /// While the capture's subscriber is the only one alive, tracing computes the interest of a callsite another
    /// thread reaches first from that thread's default alone, which records nothing, and caches it for every thread;
    /// a second dispatcher kept alive meanwhile makes it ask every live one, the capture's included.
    pub fn capture<R>(&self, f: impl FnOnce() -> R) -> R {
        let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
        tracing::subscriber::with_default(self.subscriber(), || {
            tracing::callsite::rebuild_interest_cache();
            f()
        })
    }

    /// The events recorded so far, oldest first.
    pub fn events(&self) -> Vec<CapturedEvent> {
        lock(&self.events).clone()
    }
}
