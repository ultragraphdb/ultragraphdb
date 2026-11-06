/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_protocol_scram::{MIN_ITERATIONS, ScramVerifier};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::Position;

use crate::utils::harness::{fresh, fresh_with, learn_through, sync, verifier};

#[test]
fn test_principals_are_numbered_from_1() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let (ann, position) = group.create_principal("ann", verifier("ann's password!", 1)?, true)?;
    assert_eq!((ann, position.get()), (1, 1));
    let (bob, _) = group.create_principal("bob", verifier("bob's password!", 2)?, false)?;
    assert_eq!(bob, 2);
    sync(&mut group)?;
    assert_eq!(
        group.state().principal("bob").map(|p| p.administrator()),
        Some(false)
    );
    Ok(())
}

#[test]
fn test_a_principal_is_refused_by_its_name_and_salt() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let empty = group
        .create_principal("", verifier("password here!", 1)?, false)
        .err()
        .context("refused")?;
    assert_eq!(empty.0, ClusterError::principal_name(0).0);
    let salty = ScramVerifier::new(vec![0; 256], MIN_ITERATIONS, [1; 32], [2; 32]);
    let salt = group
        .create_principal("ann", salty, false)
        .err()
        .context("refused")?;
    assert_eq!(
        salt.0,
        ClusterError::too_many("bytes of a verifier's salt", 256, 255).0
    );
    let _: (u64, Position) =
        group.create_principal("ann", verifier("ann's password!", 1)?, false)?;
    let twice = group
        .create_principal("ann", verifier("ann's password!", 1)?, false)
        .err()
        .context("refused")?;
    assert_eq!(twice.0, ClusterError::principal_exists("ann").0);
    Ok(())
}

#[test]
fn test_the_bootstrap_derives_the_verifier_of_the_files_password() -> TestResult {
    for (file, password) in [
        (&b"a long enough password\n"[..], "a long enough password"),
        (&b"a long enough password\r\n"[..], "a long enough password"),
        (&b"a long enough password"[..], "a long enough password"),
        (&b" spaces stay in it \n"[..], " spaces stay in it "),
    ] {
        let (_storage, mut group) = fresh()?;
        let (id, _) = group.bootstrap("root", file, &[4; 16], MIN_ITERATIONS)?;
        sync(&mut group)?;
        let principal = group.state().principal("root").context("root")?;
        assert_eq!((principal.id(), id), (1, 1));
        assert!(principal.administrator());
        let expected = ScramVerifier::derive(password, &[4; 16], MIN_ITERATIONS)?;
        assert_eq!(principal.verifier(), &expected);
    }
    Ok(())
}

#[test]
fn test_the_bootstrap_is_refused_naming_what_it_needs() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let cases: [(&[u8], u32, &str); 6] = [
        (&[0xFF, 0xFE], MIN_ITERATIONS, "a secret file of UTF-8 text"),
        (b"", MIN_ITERATIONS, "a password in the secret file"),
        (b"\n", MIN_ITERATIONS, "a password in the secret file"),
        (
            b"a long enough password",
            MIN_ITERATIONS - 1,
            "an iteration count from 4,096 to 10,000,000",
        ),
        // U+0007, a control character SASLprep prohibits.
        (
            b"bell \x07 in a password",
            MIN_ITERATIONS,
            "a password SASLprep accepts",
        ),
        // Only one line ending is left out: the second is the password's, a control character.
        (
            b"a long enough password\n\n",
            MIN_ITERATIONS,
            "a password SASLprep accepts",
        ),
    ];
    for (file, iterations, what) in cases {
        let refusal = group
            .bootstrap("root", file, &[1; 16], iterations)
            .err()
            .context("refused")?;
        assert_eq!(refusal.0, ClusterError::bootstrap(what).0, "{what}");
    }
    let refusal_2 = group
        .bootstrap("root", b"a long enough password", &[1; 16], 10_000_001)
        .err()
        .context("refused")?;
    assert_eq!(
        refusal_2.0,
        ClusterError::bootstrap("an iteration count from 4,096 to 10,000,000").0
    );
    let refusal_3 = group
        .bootstrap("", b"a long enough password", &[1; 16], MIN_ITERATIONS)
        .err()
        .context("refused")?;
    assert_eq!(refusal_3.0, ClusterError::principal_name(0).0);
    assert_eq!(group.applied().get(), 0);
    let _: (u64, Position) =
        group.bootstrap("root", b"a long enough password", &[1; 16], MIN_ITERATIONS)?;
    // Once a principal exists, even one not yet durable, the cluster is past its first start.
    let refusal_4 = group
        .bootstrap("other", b"another password!", &[1; 16], MIN_ITERATIONS)
        .err()
        .context("refused")?;
    assert_eq!(
        refusal_4.0,
        ClusterError::bootstrap("a group holding no principal, at the cluster's first start").0
    );
    Ok(())
}

#[test]
fn test_removing_principals() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let unknown = group.remove_principal("ann").err().context("refused")?;
    assert_eq!(unknown.0, ClusterError::unknown_principal("ann").0);
    let _: (u64, Position) =
        group.create_principal("root", verifier("root's password!", 1)?, true)?;
    let _: (u64, Position) =
        group.create_principal("ann", verifier("ann's password!", 2)?, false)?;
    let last = group.remove_principal("root").err().context("refused")?;
    assert_eq!(last.0, ClusterError::last_administrator("root").0);
    let (evicted, position) = group.remove_principal("ann")?;
    assert_eq!((evicted, position.get()), (Vec::new(), 3));
    sync(&mut group)?;
    assert!(group.state().principal("ann").is_none());
    assert_eq!(group.state().principals(), 1);
    Ok(())
}

#[test]
fn test_sessions_register_and_evict_the_oldest_past_sessions_max() -> TestResult {
    let (_storage, mut group) = fresh_with(MetadataConfig::new().with_sessions_max(2)?)?;
    let unknown = group
        .register_session(1, "ann", |_| false)
        .err()
        .context("refused")?;
    assert_eq!(unknown.0, ClusterError::unknown_principal("ann").0);
    let _: (u64, Position) =
        group.create_principal("ann", verifier("ann's password!", 1)?, false)?;
    assert!(group.register_session(1, "ann", |_| false)?.0.is_empty());
    let twice = group
        .register_session(1, "ann", |_| false)
        .err()
        .context("refused")?;
    assert_eq!(twice.0, ClusterError::session_exists(1).0);
    assert!(group.register_session(2, "ann", |_| false)?.0.is_empty());
    let (third, _) = group.register_session(3, "ann", |_| false)?;
    assert_eq!(third, vec![1]);
    let (fourth, _) = group.register_session(4, "ann", |_| false)?;
    assert_eq!(fourth, vec![2]);
    sync(&mut group)?;
    assert_eq!(group.state().sessions(), 2);
    assert_eq!(group.state().pending_evictions(), vec![1, 2]);
    // An evicted session may register again, as a new one.
    let (again, _) = group.register_session(1, "ann", |_| false)?;
    assert_eq!(again, vec![3]);
    // The next close carries the evictions to the partitions.
    sync(&mut group)?;
    learn_through(&mut group, 1)?;
    assert!(group.state().pending_evictions().is_empty());
    Ok(())
}

#[test]
fn test_a_registration_evicts_the_oldest_session_free_of_a_commit_in_flight() -> TestResult {
    let config = MetadataConfig::new().with_sessions_max(2)?;
    let (storage, mut group) = fresh_with(config)?;
    let _: (u64, Position) =
        group.create_principal("ann", verifier("ann's password!", 1)?, false)?;
    for session in [1, 2] {
        let _: (Vec<u128>, Position) = group.register_session(session, "ann", |_| false)?;
    }
    sync(&mut group)?;
    let (evicted, _) = group.register_session(3, "ann", |session| session == 1)?;
    assert_eq!(evicted, vec![2]);
    // Appended and not yet durable: the state still holds the session the registration evicts, and the group says it does not.
    assert!(group.state().session_principal(2).is_ok());
    assert!(!group.holds_session(2) && group.holds_session(1) && group.holds_session(3));
    sync(&mut group)?;
    let recovered = crate::utils::harness::crash_and_recover(&storage, config, None)?;
    for state in [group.state(), recovered.state()] {
        assert_eq!(state.sessions(), 2);
        assert!(state.session_principal(1).is_ok() && state.session_principal(3).is_ok());
        assert!(state.session_principal(2).is_err());
        assert_eq!(state.pending_evictions(), vec![2]);
    }
    Ok(())
}

#[test]
fn test_a_full_registry_of_commits_in_flight_refuses_a_registration() -> TestResult {
    let (_storage, mut group) = fresh_with(MetadataConfig::new().with_sessions_max(2)?)?;
    let _: (u64, Position) =
        group.create_principal("ann", verifier("ann's password!", 1)?, false)?;
    for session in [1, 2] {
        let _: (Vec<u128>, Position) = group.register_session(session, "ann", |_| false)?;
    }
    sync(&mut group)?;
    let refused = group
        .register_session(3, "ann", |_| true)
        .err()
        .context("refused")?;
    assert!(matches!(
        refused.0,
        ClusterErrorEnum::Full { capacity: 2, .. }
    ));
    assert!(group.holds_session(1) && group.holds_session(2) && !group.holds_session(3));
    let (evicted, _) = group.register_session(3, "ann", |session| session == 1)?;
    assert_eq!(evicted, vec![2]);
    Ok(())
}

#[test]
fn test_a_registry_past_a_lowered_sessions_max_evicts_down_to_it() -> TestResult {
    let (storage, mut group) = fresh_with(MetadataConfig::new().with_sessions_max(4)?)?;
    let _: (u64, Position) =
        group.create_principal("ann", verifier("ann's password!", 1)?, false)?;
    for session in 1..=4 {
        let _: (Vec<u128>, Position) = group.register_session(session, "ann", |_| false)?;
    }
    sync(&mut group)?;
    let config = MetadataConfig::new().with_sessions_max(2)?;
    let mut lowered = crate::utils::harness::crash_and_recover(&storage, config, None)?;
    let (evicted, _) = lowered.register_session(5, "ann", |_| false)?;
    assert_eq!(evicted, vec![1, 2, 3]);
    sync(&mut lowered)?;
    assert_eq!(lowered.state().sessions(), 2);
    Ok(())
}

#[test]
fn test_the_bootstrap_checks_the_name_before_the_file() -> TestResult {
    let (_storage, mut group) = fresh()?;
    // A file that is no UTF-8: the name is refused first, so the file is never read.
    for (name, length) in [(String::new(), 0), ("n".repeat(256), 256)] {
        let refusal = group
            .bootstrap(&name, &[0xFF], &[1; 16], MIN_ITERATIONS)
            .err()
            .context("refused")?;
        assert_eq!(refusal.0, ClusterError::principal_name(length).0);
    }
    let (id, _) = group.bootstrap(
        &"n".repeat(255),
        b"a long enough password",
        &[1; 16],
        MIN_ITERATIONS,
    )?;
    assert_eq!(id, 1);
    Ok(())
}

#[test]
fn test_removing_a_principal_evicts_its_sessions_and_replays() -> TestResult {
    let (storage, mut group) = fresh()?;
    let _: (u64, Position) =
        group.create_principal("root", verifier("root's password!", 1)?, true)?;
    let _: (u64, Position) =
        group.create_principal("ann", verifier("ann's password!", 2)?, false)?;
    for (session, principal) in [(10, "ann"), (20, "root"), (30, "ann")] {
        let (evicted, _) = group.register_session(session, principal, |_| false)?;
        assert!(evicted.is_empty());
    }
    // The last administrator stays, and so do its sessions.
    let last = group.remove_principal("root").err().context("refused")?;
    assert_eq!(last, ClusterError::last_administrator("root"));
    let (evicted, position) = group.remove_principal("ann")?;
    assert_eq!((evicted, position.get()), (vec![10, 30], 6));
    sync(&mut group)?;
    let state = group.state();
    assert_eq!(state.pending_evictions(), vec![10, 30]);
    assert_eq!(
        state.session_principal(10).err().context("evicted")?,
        ClusterError::session_evicted(10)
    );
    assert_eq!(state.session_principal(20)?, 1);
    let recovered =
        crate::utils::harness::crash_and_recover(&storage, MetadataConfig::new(), None)?;
    assert_eq!(recovered.state(), group.state());
    // The next close carries the evictions to the partitions.
    learn_through(&mut group, 1)?;
    assert_eq!(group.state().pending_evictions(), Vec::<u128>::new());
    Ok(())
}
