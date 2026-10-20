/*
 * Copyright (c) "2026" . Marvin Hansen All Rights Reserved.
 */

use tonic::{Code, Status};
use ultragraphdb_client::{
    ConnectError, ConnectionStringError, TransactionError, UltraGraphError, UltraGraphErrorEnum,
};

#[test]
fn wraps_each_component_error() {
    let err = UltraGraphError::from(ConnectionStringError::MissingPort());
    assert!(matches!(
        err.kind(),
        UltraGraphErrorEnum::ConnectionString(_)
    ));

    let err = UltraGraphError::from(ConnectError::NoEndpoints());
    assert!(matches!(err.kind(), UltraGraphErrorEnum::Connect(_)));

    let err = UltraGraphError::from(TransactionError::Finished());
    assert!(matches!(err.kind(), UltraGraphErrorEnum::Transaction(_)));
}

// Status is flattened at the boundary so this crate's errors stay Clone + PartialEq and
// no transport type reaches the public API.
#[test]
fn flattens_status_into_code_and_message() {
    let err = UltraGraphError::from(Status::new(Code::NotFound, "missing"));

    match err.kind() {
        UltraGraphErrorEnum::Rpc { code, message } => {
            assert_eq!(*code, Code::NotFound);
            assert_eq!(message, "missing");
        }
        other => panic!("unexpected classification: {other:?}"),
    }
}

#[test]
fn predicates_delegate_to_the_wrapped_error() {
    let aborted = UltraGraphError::from(TransactionError::Aborted(Code::Aborted, String::new()));
    assert!(aborted.is_aborted());
    assert!(!aborted.is_cluster_not_ready());

    let not_ready = UltraGraphError::from(ConnectError::ClusterNotReady(
        Code::Unavailable,
        "Please retry".to_string(),
    ));
    assert!(not_ready.is_cluster_not_ready());
    assert!(!not_ready.is_aborted());
}

// tonic never delivers a gRPC status for an RPC whose connection broke mid-call, so it
// synthesises `Unknown: transport error` instead. Callers must be able to tell that apart
// from a genuine `Unknown` returned by the server, which is not retryable.
#[test]
fn transport_failures_are_retryable_however_they_arrive() {
    assert!(UltraGraphError::from(ConnectError::Transport("boom".to_string())).is_transport());
    assert!(UltraGraphError::from(Status::new(Code::Unavailable, "no route")).is_transport());
    assert!(UltraGraphError::from(Status::new(Code::Unknown, "transport error")).is_transport());
}

#[test]
fn server_rejections_are_not_transport_failures() {
    // Same code, real answer from the server: retrying re-asks a question already answered.
    assert!(!UltraGraphError::from(Status::new(Code::Unknown, "schema is invalid")).is_transport());
    assert!(!UltraGraphError::from(Status::new(Code::NotFound, "missing")).is_transport());
    assert!(!UltraGraphError::from(TransactionError::Finished()).is_transport());
    assert!(
        !UltraGraphError::from(ConnectError::ClusterNotReady(
            Code::Unavailable,
            "Please retry".to_string()
        ))
        .is_transport()
    );
}

#[test]
fn display_delegates_to_the_wrapped_error() {
    let inner = ConnectionStringError::MissingPort();
    let expected = format!("{inner}");

    let err = UltraGraphError::from(inner);
    assert_eq!(format!("{err}"), expected);
}
