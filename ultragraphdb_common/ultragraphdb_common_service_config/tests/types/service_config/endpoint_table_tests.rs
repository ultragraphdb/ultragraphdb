/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service_config::{
    EndpointSecurity, ServiceConfig, ServiceConfigError, ServiceConfigErrorEnum,
};
use ultragraphdb_tool_bdd::TestResult;

fn security(text: &str) -> TestResult<Result<EndpointSecurity, ServiceConfigError>> {
    let config = ServiceConfig::parse(Some(text.as_bytes()), [])?;
    Ok(config.endpoint().security())
}

#[test]
fn test_clear_text_is_chosen_by_name() -> TestResult {
    let chosen = security("[endpoint]\nplaintext = true\n")?;
    assert_eq!(chosen, Ok(EndpointSecurity::Plaintext));
    Ok(())
}

#[test]
fn test_a_certificate_and_key_choose_tls() -> TestResult {
    let chosen =
        security("[endpoint]\ntls_cert_file = \"c\"\ntls_key_file = \"k\"\nplaintext = false\n")?;
    assert_eq!(
        chosen,
        Ok(EndpointSecurity::Tls {
            cert_file: "c".to_string(),
            key_file: "k".to_string(),
        })
    );
    Ok(())
}

#[test]
fn test_an_endpoint_deciding_nothing_is_refused() -> TestResult {
    for text in ["", "[endpoint]\nplaintext = false\n"] {
        assert_eq!(
            security(text)?,
            Err(ServiceConfigError(
                ServiceConfigErrorEnum::NoEndpointSecurity
            )),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn test_a_certificate_with_clear_text_is_refused() -> TestResult {
    for text in [
        "[endpoint]\nplaintext = true\ntls_cert_file = \"c\"\ntls_key_file = \"k\"\n",
        "[endpoint]\nplaintext = true\ntls_cert_file = \"c\"\n",
        "[endpoint]\nplaintext = true\ntls_key_file = \"k\"\n",
    ] {
        assert_eq!(
            security(text)?,
            Err(ServiceConfigError(
                ServiceConfigErrorEnum::ConflictingEndpointSecurity
            )),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn test_half_a_certificate_names_the_other_half() -> TestResult {
    let cases = [
        ("[endpoint]\ntls_cert_file = \"c\"\n", "tls_key_file"),
        ("[endpoint]\ntls_key_file = \"k\"\n", "tls_cert_file"),
    ];
    for (text, absent) in cases {
        assert_eq!(
            security(text)?,
            Err(ServiceConfigError(
                ServiceConfigErrorEnum::IncompleteEndpointSecurity { absent }
            )),
            "{text}"
        );
    }
    Ok(())
}
