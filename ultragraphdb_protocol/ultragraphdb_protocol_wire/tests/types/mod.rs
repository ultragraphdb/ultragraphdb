/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

#[cfg(test)]
mod abort;
#[cfg(test)]
mod abort_view;
#[cfg(test)]
mod action;
#[cfg(test)]
pub mod authenticate;
#[cfg(test)]
mod authenticate_view;
#[cfg(test)]
mod begin;
#[cfg(test)]
mod begin_view;
#[cfg(test)]
mod bind;
#[cfg(test)]
mod bind_kind;
#[cfg(test)]
mod bind_view;
#[cfg(test)]
mod bucket_range;
#[cfg(test)]
mod cancel;
#[cfg(test)]
mod cancel_view;
#[cfg(test)]
pub mod catalog;
#[cfg(test)]
mod catalog_announcement;
#[cfg(test)]
mod catalog_view;
#[cfg(test)]
mod challenge;
#[cfg(test)]
mod challenge_view;
#[cfg(test)]
mod chunk_layout;
#[cfg(test)]
mod chunked_body;
#[cfg(test)]
mod close_request;
#[cfg(test)]
mod close_request_view;
#[cfg(test)]
mod closed;
#[cfg(test)]
mod closed_partition;
#[cfg(test)]
mod closed_view;
#[cfg(test)]
mod column_type;
#[cfg(test)]
mod command;
#[cfg(test)]
pub mod commit;
#[cfg(test)]
mod commit_operation;
#[cfg(test)]
mod commit_view;
#[cfg(test)]
mod compute_read;
#[cfg(test)]
mod compute_read_view;
#[cfg(test)]
pub mod conditional_write;
#[cfg(test)]
mod conditional_write_view;
#[cfg(test)]
mod conflict_kind;
#[cfg(test)]
mod credit;
#[cfg(test)]
mod credit_view;
#[cfg(test)]
mod decision;
#[cfg(test)]
mod declared_entity_type;
#[cfg(test)]
mod declared_predicate;
#[cfg(test)]
mod declared_relation;
#[cfg(test)]
mod default_entry;
#[cfg(test)]
mod default_entry_view;
#[cfg(test)]
pub mod describe;
#[cfg(test)]
mod describe_view;
#[cfg(test)]
pub mod direct_write;
#[cfg(test)]
pub mod direct_write_view;
#[cfg(test)]
pub mod drop_command;
#[cfg(test)]
mod drop_command_view;
#[cfg(test)]
mod drop_kind;
#[cfg(test)]
pub mod entries;
#[cfg(test)]
mod entries_end;
#[cfg(test)]
mod entries_sequence;
#[cfg(test)]
mod entries_view;
#[cfg(test)]
pub mod epoch_close;
#[cfg(test)]
mod epoch_close_view;
#[cfg(test)]
mod error_code;
#[cfg(test)]
mod error_reply;
#[cfg(test)]
mod error_reply_view;
#[cfg(test)]
pub mod execute;
#[cfg(test)]
mod execute_view;
#[cfg(test)]
mod frame_view;
#[cfg(test)]
mod header;
#[cfg(test)]
mod header_view;
#[cfg(test)]
mod held;
#[cfg(test)]
mod held_view;
#[cfg(test)]
mod hello;
#[cfg(test)]
mod hello_view;
#[cfg(test)]
pub mod install;
#[cfg(test)]
mod install_view;
#[cfg(test)]
pub mod join;
#[cfg(test)]
mod join_view;
#[cfg(test)]
mod key_values;
#[cfg(test)]
mod language;
#[cfg(test)]
pub mod lease;
#[cfg(test)]
mod lease_view;
#[cfg(test)]
mod link;
#[cfg(test)]
mod member;
#[cfg(test)]
pub mod members;
#[cfg(test)]
mod members_view;
#[cfg(test)]
mod monotonic;
#[cfg(test)]
mod object_record;
#[cfg(test)]
mod object_records;
#[cfg(test)]
mod object_records_builder;
#[cfg(test)]
pub mod operation;
#[cfg(test)]
mod operation_result;
#[cfg(test)]
mod operation_view;
#[cfg(test)]
mod pattern_slot;
#[cfg(test)]
pub mod placement;
#[cfg(test)]
mod placement_view;
#[cfg(test)]
mod preferred_leader;
#[cfg(test)]
pub mod prepare;
#[cfg(test)]
mod prepare_view;
#[cfg(test)]
mod quad_pattern;
#[cfg(test)]
pub mod query;
#[cfg(test)]
mod query_view;
#[cfg(test)]
pub mod read;
#[cfg(test)]
mod read_view;
#[cfg(test)]
mod recall;
#[cfg(test)]
mod recall_view;
#[cfg(test)]
mod record_entry;
#[cfg(test)]
mod record_kind;
#[cfg(test)]
mod record_operation;
#[cfg(test)]
mod record_registration;
#[cfg(test)]
pub mod reference;
#[cfg(test)]
mod reference_registration;
#[cfg(test)]
mod reference_view;
#[cfg(test)]
mod register;
#[cfg(test)]
mod register_view;
#[cfg(test)]
pub mod report;
#[cfg(test)]
mod report_kind;
#[cfg(test)]
mod report_view;
#[cfg(test)]
mod request_term_view;
#[cfg(test)]
pub mod result_batch;
#[cfg(test)]
mod result_term;
#[cfg(test)]
mod result_term_view;
#[cfg(test)]
mod result_view;
#[cfg(test)]
mod role_kind;
#[cfg(test)]
pub mod round_body;
#[cfg(test)]
mod round_header;
#[cfg(test)]
mod round_view;
#[cfg(test)]
mod scan;
#[cfg(test)]
mod scan_at;
#[cfg(test)]
mod scan_range;
#[cfg(test)]
mod scan_view;
#[cfg(test)]
mod statement_cache;
#[cfg(test)]
mod string_list;
#[cfg(test)]
pub mod subscribe;
#[cfg(test)]
mod subscribe_view;
#[cfg(test)]
mod tenant_run;
#[cfg(test)]
mod term_table;
#[cfg(test)]
pub mod term_table_view;
#[cfg(test)]
mod verify;
#[cfg(test)]
mod verify_view;
#[cfg(test)]
mod welcome;
#[cfg(test)]
mod welcome_view;
#[cfg(test)]
pub mod write;
#[cfg(test)]
pub mod write_set_record;
#[cfg(test)]
pub mod write_set_record_view;
#[cfg(test)]
mod write_view;
