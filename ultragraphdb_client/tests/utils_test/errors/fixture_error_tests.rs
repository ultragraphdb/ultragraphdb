use ultragraphdb_client::utils_test::{FixtureError, FixtureErrorEnum};

#[test]
fn unsupported_strategy_names_the_supported_ones() {
    let err = FixtureError::new(FixtureErrorEnum::UnsupportedStrategy {
        value: "localhost".to_string(),
    });
    let text = err.to_string();

    // The reader's next question is always "then which ones work", so the message answers it.
    assert!(text.contains("localhost"), "{text}");
    assert!(text.contains("container"), "{text}");
    assert!(text.contains("existing"), "{text}");
    assert!(err.is_unsupported_strategy());
    assert!(!err.is_not_ready());
}

/// A Dgraph this process did not create has a password somebody chose, so there is nothing
/// sensible to default to. The message has to say both which variable is missing and what the
/// alternative is, because "unset" alone leaves the reader guessing.
#[test]
fn a_missing_credential_names_the_variable_and_the_way_out() {
    let err = FixtureError::new(FixtureErrorEnum::MissingCredential {
        variable: "DGRAPH_ADMIN_PASSWORD".to_string(),
    });
    let text = err.to_string();

    assert!(text.contains("DGRAPH_ADMIN_PASSWORD"), "{text}");
    assert!(text.contains("container"), "{text}");
    assert!(err.is_missing_credential());
}

#[test]
fn not_ready_carries_enough_to_diagnose() {
    let err = FixtureError::new(FixtureErrorEnum::NotReady {
        host: "alpha.example".to_string(),
        port: 9080,
        attempts: 7,
        last: "connection refused".to_string(),
    });
    let text = err.to_string();

    assert!(text.contains("alpha.example"), "{text}");
    assert!(text.contains("9080"), "{text}");
    assert!(text.contains('7'), "{text}");
    assert!(text.contains("connection refused"), "{text}");
    assert!(err.is_not_ready());
}

#[test]
fn kind_is_the_branching_surface() {
    let err = FixtureError::new(FixtureErrorEnum::Docker("no daemon".to_string()));
    assert!(matches!(err.kind(), FixtureErrorEnum::Docker(_)));
}
