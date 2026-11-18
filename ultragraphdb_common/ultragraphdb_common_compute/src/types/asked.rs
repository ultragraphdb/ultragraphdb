/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;

use crate::types::link::Link;

/// A stream of a link asked one question, forgotten however its wait ends, a read cancelled meanwhile included.
pub(crate) struct Asked {
    pub(crate) link: Rc<Link>,
    pub(crate) stream: u64,
}

impl Drop for Asked {
    fn drop(&mut self) {
        self.link.forget(self.stream);
    }
}
