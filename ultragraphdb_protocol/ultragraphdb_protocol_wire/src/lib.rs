/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The binary protocol's bytes: the 256-byte header, XXH3-64 checksums and chunk tables, the commands, and every
//! body's layout with its view and builder. No I/O.
//!
//! A frame is read in the order the protocol checks it: [`HeaderView::new`] takes the header's 256 bytes (the
//! length), [`HeaderView::verify`] checks its checksum, command, reserved bytes, cluster and size, and
//! [`FrameView::validate`] does both and then checks the body's chunk table and chunks. A frame is built with
//! [`Header::encode_frame`], which appends the chunk table and fills in `size` and the checksums.
//!
//! A body is read through its command's view, a [`BodyView`] constructed from the validated frame, which checks the
//! command header's fields and the body's structure once: a section table of u32 offsets and lengths, sections
//! aligned to 8 bytes, counts, UTF-8 text and terms; every value is then read in place. A body is built from its
//! owned type, a [`BodyBuilder`], whose [`BodyBuilder::frame`] gives the frame. Terms a client sends are RDF terms in
//! N-Triples text; terms a server returns are `ultragraphdb_core_terms`' encodings, in a result batch's term table.

mod errors;
mod traits;
mod types;
mod utils;

pub use crate::errors::statement_error::{StatementError, StatementErrorEnum};
pub use crate::traits::body_builder::BodyBuilder;
pub use crate::traits::body_view::BodyView;
pub use crate::types::abort::Abort;
pub use crate::types::abort_view::AbortView;
pub use crate::types::begin::Begin;
pub use crate::types::begin_view::BeginView;
pub use crate::types::cancel::Cancel;
pub use crate::types::cancel_view::CancelView;

pub use crate::types::chunk_layout::ChunkLayout;
pub use crate::types::chunked_body::ChunkedBody;
pub use crate::types::column_type::ColumnType;
pub use crate::types::command::Command;
pub use crate::types::commit::Commit;
pub use crate::types::commit_operation::CommitOperation;
pub use crate::types::commit_view::CommitView;
pub use crate::types::conflict_kind::ConflictKind;
pub use crate::types::credit::Credit;
pub use crate::types::credit_view::CreditView;
pub use crate::types::decision::Decision;
pub use crate::types::declared::Declared;
pub use crate::types::declared_entity_type::DeclaredEntityType;
pub use crate::types::declared_kind::DeclaredKind;
pub use crate::types::declared_predicate::DeclaredPredicate;
pub use crate::types::declared_relation::DeclaredRelation;
pub use crate::types::describe::Describe;
pub use crate::types::describe_view::DescribeView;
pub use crate::types::direction::Direction;
pub use crate::types::error_code::ErrorCode;
pub use crate::types::error_reply::ErrorReply;
pub use crate::types::error_reply_view::ErrorReplyView;
pub use crate::types::execute::Execute;
pub use crate::types::execute_view::ExecuteView;
pub use crate::types::frame_view::FrameView;
pub use crate::types::header::Header;
pub use crate::types::header_view::HeaderView;
pub use crate::types::hello::Hello;
pub use crate::types::hello_view::HelloView;
pub use crate::types::key_values::KeyValues;
pub use crate::types::link::Link;
pub use crate::types::operation::Operation;
pub use crate::types::operation_result::OperationResult;
pub use crate::types::operation_view::OperationView;
pub use crate::types::pattern_slot::PatternSlot;
pub use crate::types::prepare::Prepare;
pub use crate::types::prepare_view::PrepareView;
pub use crate::types::quad_pattern::QuadPattern;
pub use crate::types::read::Read;
pub use crate::types::read_view::ReadView;
pub use crate::types::record_entry::RecordEntry;
pub use crate::types::record_operation::RecordOperation;
pub use crate::types::register::Register;
pub use crate::types::register_view::RegisterView;
pub use crate::types::request_term::RequestTerm;
pub use crate::types::request_term_view::RequestTermView;
pub use crate::types::result_batch::ResultBatch;
pub use crate::types::result_term::ResultTerm;
pub use crate::types::result_term_view::ResultTermView;
pub use crate::types::result_view::ResultView;
pub use crate::types::round_body::RoundBody;
pub use crate::types::round_header::RoundHeader;
pub use crate::types::round_view::RoundView;
pub use crate::types::statement_cache::StatementCache;
pub use crate::types::statement_target::StatementTarget;
pub use crate::types::string_list::StringList;
pub use crate::types::term_table::TermTable;
pub use crate::types::term_table_view::TermTableView;
pub use crate::types::welcome::Welcome;
pub use crate::types::welcome_view::WelcomeView;
pub use crate::types::write::Write;
pub use crate::types::write_change::WriteChange;
pub use crate::types::write_set_record::{OwnedEntry, WriteSetRecord};
pub use crate::types::write_set_record_view::WriteSetRecordView;
pub use crate::types::write_view::WriteView;
pub use crate::utils::checksum::{xxh3_64, xxh3_64_with_seed};
pub use crate::utils::constants::{
    ABSENT, CHUNK_SIZE, COMMAND_HEADER_SIZE, FIRST_CHUNK_SIZE, FLAG_LAST, HEADER_SIZE, LEAF_SIZE,
    MAX_FRAME_SIZE, MAX_TERM_DEPTH, PROTOCOL_VERSION, REPLICA_COMMANDS, RETIRED_COMMANDS,
};
