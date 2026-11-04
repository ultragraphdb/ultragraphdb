/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

/// One thing wrong with settings, complete on its own: where it stands, which target, instance and setting, what was found
/// and what is required.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Defect {
    file: Option<String>,
    line: Option<usize>,
    target: Option<String>,
    revision: Option<u64>,
    instance: Option<String>,
    setting: Option<String>,
    found: String,
    required: String,
}

impl Defect {
    /// A defect of `found`, where `required` holds instead.
    pub fn new(found: impl Into<String>, required: impl Into<String>) -> Self {
        Self {
            file: None,
            line: None,
            target: None,
            revision: None,
            instance: None,
            setting: None,
            found: found.into(),
            required: required.into(),
        }
    }

    /// The defect standing in `file`.
    #[must_use]
    pub fn in_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// The defect standing on `line`, when a parser knows it.
    #[must_use]
    pub fn on_line(mut self, line: Option<usize>) -> Self {
        self.line = line;
        self
    }

    /// The defect of `target`.
    #[must_use]
    pub fn of_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// The defect of `revision`, when it stands in a stored one.
    #[must_use]
    pub fn of_revision(mut self, revision: u64) -> Self {
        self.revision = Some(revision);
        self
    }

    /// The defect of `instance`.
    #[must_use]
    pub fn of_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    /// The defect of `setting`, `table.key`.
    #[must_use]
    pub fn of_setting(mut self, setting: impl Into<String>) -> Self {
        self.setting = Some(setting.into());
        self
    }

    /// The file it stands in.
    #[must_use]
    pub fn file(&self) -> Option<&str> {
        self.file.as_deref()
    }

    /// The line it stands on.
    #[must_use]
    pub fn line(&self) -> Option<usize> {
        self.line
    }

    /// The setting.
    #[must_use]
    pub fn setting(&self) -> Option<&str> {
        self.setting.as_deref()
    }
}

impl fmt::Display for Defect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.file {
            write!(f, "{file}")?;
            if let Some(line) = self.line {
                write!(f, ":{line}")?;
            }
            f.write_str(": ")?;
        }
        if let Some(target) = &self.target {
            write!(f, "target {target}, ")?;
        }
        if let Some(revision) = self.revision {
            write!(f, "revision {revision}, ")?;
        }
        if let Some(instance) = &self.instance {
            write!(f, "instance {instance}, ")?;
        }
        if let Some(setting) = &self.setting {
            write!(f, "setting {setting}: ")?;
        }
        write!(f, "{}; {}", self.found, self.required)
    }
}

impl std::error::Error for Defect {}
