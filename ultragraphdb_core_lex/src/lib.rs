/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `lex`: the state-function lexer, item iteration, IRI checks and quoted strings.
//!
//! A client lexer (DQL, schema, RDF) declares its item types as [`ItemType`] constants from 5 up and writes its
//! states as plain functions `fn(&mut Lexer) -> Option<StateFn>`, wrapped in [`StateFn::new`]. [`Lexer::run`] calls
//! them until one returns `None`; the parser then walks the emitted items with an [`ItemIterator`].
//!
//! ```
//! use ultragraphdb_core_lex::{ITEM_EOF, ItemType, Lexer, StateFn};
//!
//! const ITEM_WORD: ItemType = ItemType::new(5);
//!
//! fn lex_words(l: &mut Lexer) -> Option<StateFn> {
//!     loop {
//!         match l.next() {
//!             None => {
//!                 l.emit(ITEM_EOF);
//!                 return None;
//!             }
//!             Some(' ') => l.ignore(),
//!             Some(_) => {
//!                 l.accept_run(|r| r != ' ');
//!                 l.emit(ITEM_WORD);
//!             }
//!         }
//!     }
//! }
//!
//! let mut l = Lexer::new("hello world");
//! l.run(StateFn::new(lex_words));
//! assert!(l.validate_result().is_ok());
//! let mut it = l.new_iterator();
//! let mut words = Vec::new();
//! while it.next() && it.item().typ() == ITEM_WORD {
//!     words.push(it.item().val());
//! }
//! assert_eq!(words, ["hello", "world"]);
//! ```

mod errors;
mod types;
mod utils;

pub use crate::errors::lex_error::{LexError, LexErrorEnum};
pub use crate::types::item::Item;
pub use crate::types::item_iterator::ItemIterator;
pub use crate::types::item_type::{ITEM_EOF, ITEM_ERROR, ItemType};
pub use crate::types::lexer::Lexer;
pub use crate::types::state_fn::StateFn;
pub use crate::utils::iri::{has_u_chars, has_x_chars, iri_ref, is_hex, is_iri_ref_char};
pub use crate::utils::lexer::{is_end_of_line, is_esc_char};
