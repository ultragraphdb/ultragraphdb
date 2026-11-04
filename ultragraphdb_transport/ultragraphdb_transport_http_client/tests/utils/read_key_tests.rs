/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transport_http_client::read_key;

fn holding(name: &str, bytes: &[u8]) -> TestResult<String> {
    let file = std::env::temp_dir().join(format!("client-key-{name}-{}", std::process::id()));
    std::fs::write(&file, bytes)?;
    Ok(file.display().to_string())
}

/// A key is its file without a final line break.
#[test]
fn test_a_key_is_its_file() -> TestResult {
    for (name, bytes) in [
        ("bare", &b"key"[..]),
        ("newline", b"key\n"),
        ("crlf", b"key\r\n"),
    ] {
        assert_eq!(read_key(&holding(name, bytes)?)?, "key", "{name}");
    }
    Ok(())
}

/// A file absent, empty, or holding what a header cannot carry is refused naming the file.
#[test]
fn test_a_key_that_cannot_be_used_names_its_file() -> TestResult {
    let files = [
        "/nonexistent/ultragraphdb/key".to_string(),
        holding("empty", b"")?,
        holding("only-break", b"\n")?,
        holding("control", b"a\r\nX-Injected: 1")?,
        holding("binary", &[0xff])?,
    ];
    for file in files {
        let refused = read_key(&file).err().context("a refusal")?;
        assert!(refused.to_string().contains(&file), "{refused}");
    }
    Ok(())
}
