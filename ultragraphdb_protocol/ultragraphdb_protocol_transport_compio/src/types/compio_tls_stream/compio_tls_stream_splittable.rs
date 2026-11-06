/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::rc::Rc;

use compio::io::util::Splittable;

use super::CompioTlsStream;
use crate::types::tls_read_half::TlsReadHalf;
use crate::types::tls_write_half::TlsWriteHalf;

impl<S> Splittable for CompioTlsStream<S>
where
    S: Splittable,
{
    type ReadHalf = TlsReadHalf<S::ReadHalf>;
    type WriteHalf = TlsWriteHalf<S::WriteHalf>;

    fn split(self) -> (Self::ReadHalf, Self::WriteHalf) {
        let (read, write) = self.stream.split();
        let session = Rc::new(RefCell::new(self.session));
        (
            TlsReadHalf::new(Rc::clone(&session), read, self.incoming, self.plaintext),
            TlsWriteHalf::new(session, write, self.outgoing),
        )
    }
}
