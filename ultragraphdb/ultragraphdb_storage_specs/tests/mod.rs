/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A declared configuration is one the role would accept.

use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_storage_specs::{OBSERVABILITY, settings};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_the_settings_are_ones_the_configuration_takes() -> TestResult {
    let config = ServiceConfig::parse(ultragraphdb_frontend_specs::render(&settings()).as_bytes())?;
    let address = config.observability().address().map(ToString::to_string);
    assert_eq!(address, Some(format!("tcp://{OBSERVABILITY}")));
    Ok(())
}
