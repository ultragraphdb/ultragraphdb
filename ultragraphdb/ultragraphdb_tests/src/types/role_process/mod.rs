/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs::File;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::Duration;

use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::utils::http_status::http_status;

/// Where the sandbox holds the binaries, relative to a test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_tests";

/// How long a role has to answer its first probe. A start takes milliseconds; the margin is for a loaded machine.
const START_TIME: Duration = Duration::from_secs(20);
const PROBE_INTERVAL: Duration = Duration::from_millis(20);

/// A role binary running as a process, stopped when this is dropped, its log in a file a test can read.
#[derive(Debug)]
pub struct RoleProcess {
    child: Child,
    log: PathBuf,
}

impl RoleProcess {
    /// Starts the binary named `binary`, from the sandbox, with `args` and `variables` alone in its
    /// environment, so nothing of the test's own reaches its settings.
    ///
    /// # Errors
    ///
    /// The binary cannot be started, or its log cannot be created.
    pub fn start(binary: &str, args: &[&str], variables: &[(String, String)]) -> TestResult<Self> {
        let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
        let log = root.join(format!("{binary}-{}.log", std::process::id()));
        let child = Command::new(format!("{ROOT_PATH}/{binary}"))
            .args(args)
            .env_clear()
            .envs(variables.iter().map(|(key, value)| (key, value)))
            .stdout(Stdio::null())
            .stderr(File::create(&log)?)
            .spawn()?;
        Ok(Self { child, log })
    }

    /// Waits until `address` answers `path` with 200.
    ///
    /// # Errors
    ///
    /// The process exits first, or nothing answers within the start's time, the log's text in the error.
    pub fn wait_for(&mut self, address: &str, path: &str) -> TestResult {
        for _ in 0..START_TIME.as_millis() / PROBE_INTERVAL.as_millis() {
            if let Some(status) = self.child.try_wait()? {
                return Err(TestError::new(format!("exited {status}: {}", self.log()?)));
            }
            if matches!(http_status(address, path), Ok(200)) {
                return Ok(());
            }
            std::thread::sleep(PROBE_INTERVAL);
        }
        Err(TestError::new(format!(
            "{address}{path} never answered: {}",
            self.log()?
        )))
    }

    /// Waits for the process to exit by itself.
    ///
    /// # Errors
    ///
    /// It does not exit within the start's time.
    pub fn wait_exit(&mut self) -> TestResult<ExitStatus> {
        for _ in 0..START_TIME.as_millis() / PROBE_INTERVAL.as_millis() {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status);
            }
            std::thread::sleep(PROBE_INTERVAL);
        }
        Err(TestError::new("the process did not exit"))
    }

    /// Sends SIGTERM and waits for the process to exit.
    ///
    /// # Errors
    ///
    /// The signal cannot be sent, or it does not exit.
    pub fn terminate(&mut self) -> TestResult<ExitStatus> {
        let pid = self.child.id().to_string();
        let sent = Command::new("kill").args(["-TERM", &pid]).status()?;
        if !sent.success() {
            return Err(TestError::new(format!("kill answered {sent}")));
        }
        self.wait_exit()
    }

    /// What the process has logged so far.
    ///
    /// # Errors
    ///
    /// The log cannot be read.
    pub fn log(&self) -> TestResult<String> {
        Ok(std::fs::read_to_string(&self.log)?)
    }
}

impl Drop for RoleProcess {
    fn drop(&mut self) {
        // A test that failed leaves its process running otherwise, holding its declared port against the next.
        let _killed = self.child.kill();
        let _reaped = self.child.wait();
        let _removed = std::fs::remove_file(&self.log);
    }
}
