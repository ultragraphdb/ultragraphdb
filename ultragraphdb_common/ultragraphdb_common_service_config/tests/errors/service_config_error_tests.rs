/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service_config::{ServiceConfigError, ServiceConfigErrorEnum};

/// Every refusal names what an operator must fix, in the message a role prints before it exits.
#[test]
fn test_each_refusal_names_what_it_refuses() {
    let cases = [
        (ServiceConfigError::unreadable("/etc/x", "gone"), "/etc/x"),
        (ServiceConfigError::unknown_table("storge"), "storge"),
        (
            ServiceConfigError::unknown_variable("ULTRAGRAPHDB_X"),
            "ULTRAGRAPHDB_X",
        ),
        (
            ServiceConfigError::wrong_kind_variable("ULTRAGRAPHDB_Y", "many", "an integer"),
            "ULTRAGRAPHDB_Y",
        ),
        (
            ServiceConfigError::out_of_range("storage", "partition_count", -1, 7),
            "storage.partition_count",
        ),
        (
            ServiceConfigError::missing("cluster", "role_secret_file"),
            "cluster.role_secret_file",
        ),
        (
            ServiceConfigError(ServiceConfigErrorEnum::NoEndpointSecurity),
            "plaintext",
        ),
        (
            ServiceConfigError(ServiceConfigErrorEnum::ConflictingEndpointSecurity),
            "plaintext",
        ),
        (
            ServiceConfigError(ServiceConfigErrorEnum::IncompleteEndpointSecurity {
                absent: "tls_key_file",
            }),
            "tls_key_file",
        ),
    ];
    for (error, named) in cases {
        let message = error.to_string();
        assert!(message.contains(named), "{message} names {named}");
        let source: &dyn std::error::Error = &error;
        assert!(source.source().is_none());
    }
}
