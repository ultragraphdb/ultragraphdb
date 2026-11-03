/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{Read, Write};

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transport_tls::TestEcho;

#[test]
fn test_an_echo_answers_what_it_was_written_once_and_in_order() -> TestResult {
    let mut echo = TestEcho::default();
    let mut buf = [0u8; 4];
    assert_eq!(echo.read(&mut buf).context("a read of nothing")?, 0);
    echo.write_all(b"abc").context("a write")?;
    echo.write_all(b"def").context("a write")?;
    echo.flush().context("a flush")?;
    assert_eq!(echo.read(&mut buf).context("a read")?, 4);
    assert_eq!(&buf, b"abcd");
    assert_eq!(echo.read(&mut buf).context("a read")?, 2);
    assert_eq!(buf.get(..2), Some(&b"ef"[..]));
    assert_eq!(echo.read(&mut buf).context("a read of nothing")?, 0);
    Ok(())
}
