use ultragraphdb_client::utils_test::{Exclusivity, Strategy};

/// `Strategy::parse` rather than `Strategy::from_env` throughout this file.
///
/// The environment is process-global and Rust runs test functions concurrently, so a test that
/// sets a variable to observe `from_env` races every other test in the binary. `from_env` is a
/// two-line wrapper over `parse` plus a default; `parse` is where the decision lives.
#[test]
fn container_provisions_and_owns() {
    let strategy = Strategy::parse("container").expect("container is a strategy");
    assert_eq!(strategy, Strategy::Container);
    assert_eq!(strategy.exclusivity(), Exclusivity::Exclusive);
}

#[test]
fn existing_verifies_and_does_not_own() {
    let strategy = Strategy::parse("existing").expect("existing is a strategy");
    assert_eq!(strategy, Strategy::Existing);
    // A Dgraph this process did not create is shared with whoever else is using it.
    assert_eq!(strategy.exclusivity(), Exclusivity::Shared);
}

/// The default is what an unset variable resolves to, so it is part of the contract rather than
/// an implementation detail: a workstation with a Docker daemon needs no configuration at all.
#[test]
fn the_default_is_container() {
    assert_eq!(Strategy::default(), Strategy::Container);
}

/// An unknown value is REFUSED rather than defaulted.
///
/// Defaulting a misspelled `existng` to `container` would start a container against a
/// configuration that meant to verify a shared Dgraph, and the run would pass having tested the
/// wrong thing.
#[test]
fn an_unknown_strategy_is_refused_not_defaulted() {
    for value in ["existng", "ci", "localhost", "Container", ""] {
        let err = Strategy::parse(value).expect_err("must not resolve to a strategy");
        assert!(err.is_unsupported_strategy(), "{value}: {err}");
        // The reader's next question is always "then which ones work", so the message answers it.
        assert!(err.to_string().contains("container"), "{value}: {err}");
        assert!(err.to_string().contains("existing"), "{value}: {err}");
    }
}

#[test]
fn as_str_round_trips_through_parse() {
    for strategy in [Strategy::Container, Strategy::Existing] {
        assert_eq!(Strategy::parse(strategy.as_str()), Ok(strategy));
    }
}
