/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod access_tests;
#[cfg(test)]
mod asked_credentials_tests;
mod auth_throttle_tests;
#[cfg(test)]
mod check_round_trips_tests;
#[cfg(test)]
mod client_session_tests;
mod compute_core;
#[cfg(test)]
mod endpoint_view_tests;
mod link_tests;
mod lookup_at_tests;
mod names_tests;
#[cfg(test)]
mod one_frame_phases_tests;
mod page_tests;
mod partition_rows;
mod range_spec_tests;
mod read_at_tests;
mod read_executor_tests;
mod read_pattern_tests;
mod read_pins_tests;
mod read_request_tests;
mod read_route_tests;
mod read_times_tests;
#[cfg(test)]
mod relay_authenticator_tests;
mod result_sink_tests;
mod resume_terms_tests;
#[cfg(test)]
mod role_links_tests;
mod row_terms_tests;
mod scan_stream_tests;
#[cfg(test)]
mod system_link_tests;
#[cfg(test)]
mod system_turns_tests;
mod tenant_gate_tests;
#[cfg(test)]
mod throttle_handle_tests;
mod wanted_names_tests;
