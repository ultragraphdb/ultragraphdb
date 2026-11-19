/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Lets every other task of the core run once, waiting on nothing.
/// A dropped future leaves nothing.
pub(crate) async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(|context| {
        if yielded {
            return std::task::Poll::Ready(());
        }
        yielded = true;
        context.waker().wake_by_ref();
        std::task::Poll::Pending
    })
    .await;
}
