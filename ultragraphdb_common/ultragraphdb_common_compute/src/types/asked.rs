/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;

use crate::types::partition_link::PartitionLink;

/// A stream of a link asked one question, forgotten however its wait ends, a read cancelled meanwhile included.
pub(crate) struct Asked {
    pub(crate) link: Rc<PartitionLink>,
    pub(crate) stream: u64,
}

impl Drop for Asked {
    fn drop(&mut self) {
        self.link.forget(self.stream);
    }
}
