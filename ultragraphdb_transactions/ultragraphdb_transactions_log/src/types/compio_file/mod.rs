/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod compio_file_durable_file;

/// A file of the [`crate::CompioStorage`].
#[derive(Debug)]
pub struct CompioFile {
    file: compio::fs::File,
    aligned: bool,
}

impl CompioFile {
    pub(crate) fn new(file: compio::fs::File, aligned: bool) -> Self {
        Self { file, aligned }
    }
}
