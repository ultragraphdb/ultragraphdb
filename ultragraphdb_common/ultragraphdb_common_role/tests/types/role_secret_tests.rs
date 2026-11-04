/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use ultragraphdb_common_role::RoleSecret;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_scram::Credentials;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A file holding `bytes`, named for its test.
fn holding(name: &str, bytes: &[u8]) -> TestResult<PathBuf> {
    let file = std::env::temp_dir().join(format!("role-secret-{name}-{}", std::process::id()));
    std::fs::write(&file, bytes)?;
    Ok(file)
}

fn read(file: &std::path::Path) -> Result<RoleSecret, RoleError> {
    RoleSecret::read(&file.display().to_string())
}

/// The password is the file's contents without a final line break, and only a final one.
#[test]
fn test_a_final_line_break_is_not_the_secret() -> TestResult {
    for (name, bytes, expected) in [
        ("bare", &b"s3cret"[..], "s3cret"),
        ("newline", &b"s3cret\n"[..], "s3cret"),
        ("crlf", &b"s3cret\r\n"[..], "s3cret"),
        ("two", &b"s3cret\n\n"[..], "s3cret\n"),
    ] {
        let file = holding(name, bytes)?;
        assert_eq!(read(&file)?.password(), expected, "{name}");
    }
    Ok(())
}

/// A file missing, empty or not text stops the start with the configuration's code, naming the file.
#[test]
fn test_a_secret_that_cannot_be_read_is_a_configuration_error() -> TestResult {
    let missing = std::env::temp_dir().join("role-secret-never-written");
    let files = [
        missing,
        holding("empty", b"")?,
        holding("only-break", b"\n")?,
        holding("binary", &[0xff, 0xfe])?,
    ];
    for file in files {
        let refused = read(&file).err().context("a refusal")?;
        assert_eq!(
            refused.exit_code(),
            RoleError::EX_CONFIG,
            "{}",
            file.display()
        );
        assert!(
            refused.to_string().contains(&file.display().to_string()),
            "{refused}"
        );
    }
    Ok(())
}

/// A secret's `Debug` holds where it came from and never the secret.
#[test]
fn test_a_secret_never_prints() -> TestResult {
    let file = holding("debug", b"do-not-print")?;
    let printed = format!("{:?}", read(&file)?);
    assert!(!printed.contains("do-not-print"), "{printed}");
    assert!(printed.contains(&file.display().to_string()), "{printed}");
    Ok(())
}

/// The three roles' names verify against the secret, acting for principal 0, each role's salt its own; any other name
/// is unknown.
#[test]
fn test_credentials_know_the_roles_alone() -> TestResult {
    let file = holding("credentials", b"s3cret")?;
    let secret = read(&file)?;
    let credentials = secret.credentials()?;
    for name in ["frontend", "compute", "storage"] {
        let (principal, verifier) = credentials.lookup(name).context("a role's name")?;
        assert_eq!(principal, 0, "{name}");
        assert_eq!(verifier.salt().len(), 16, "{name}");
        assert_eq!(verifier.iterations(), 4_096, "{name}");
    }
    assert!(credentials.lookup("alice").is_none());
    let (_, one) = credentials.lookup("storage").context("storage")?;
    let (_, other) = secret.credentials()?.lookup("storage").context("storage")?;
    assert_ne!(one.salt(), other.salt());
    Ok(())
}
