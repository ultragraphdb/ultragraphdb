/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod welcome_view_body_view;

/// The body of a received Welcome frame, read in place; see [`crate::Welcome`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WelcomeView<'a> {
    protocol: u16,
    release: u32,
    max_frame: u32,
    max_streams: u32,
    initial_credit: u32,
    server: &'a str,
}

impl<'a> WelcomeView<'a> {
    pub fn protocol(&self) -> u16 {
        self.protocol
    }

    pub fn release(&self) -> u32 {
        self.release
    }

    pub fn max_frame(&self) -> u32 {
        self.max_frame
    }

    pub fn max_streams(&self) -> u32 {
        self.max_streams
    }

    pub fn initial_credit(&self) -> u32 {
        self.initial_credit
    }

    pub fn server(&self) -> &'a str {
        self.server
    }
}
