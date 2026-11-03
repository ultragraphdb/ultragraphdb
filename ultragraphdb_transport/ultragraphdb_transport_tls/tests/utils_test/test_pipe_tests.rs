/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{ErrorKind, Read, Write};

use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::TestPipe;

#[test]
fn test_a_pipe_reads_what_the_other_end_wrote_and_would_block_otherwise() -> TestResult {
    let (mut near, mut far) = TestPipe::pair();
    let mut buf = [0u8; 4];
    let Err(error) = far.read(&mut buf) else {
        return Err(TestError::new("an empty pipe gave bytes"));
    };
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    near.write_all(b"abcdef").context("a write")?;
    near.flush().context("a flush")?;
    assert_eq!(far.read(&mut buf).context("a read")?, 4);
    assert_eq!(&buf, b"abcd");
    assert_eq!(far.read(&mut buf).context("a read")?, 2);
    assert_eq!(buf.get(..2), Some(&b"ef"[..]));
    assert!(near.read(&mut buf).is_err(), "nothing came the other way");
    far.write_all(b"z").context("a write back")?;
    assert_eq!(near.read(&mut buf).context("a read")?, 1);
    Ok(())
}
