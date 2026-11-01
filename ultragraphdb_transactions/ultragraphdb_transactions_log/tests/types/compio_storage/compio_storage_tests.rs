/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The compio storage's `O_DIRECT` path on Linux: an aligned file is opened with `O_DIRECT` and a plain one without,
//! as the kernel reports the open file's flags in `/proc/self/fdinfo` (proc(5)).

use std::path::Path;

use ultragraphdb_transactions_log::{CompioStorage, DurableStorage, OpenMode};

use crate::utils::helpers::{TempDir, run};

/// `O_DIRECT` as Linux defines it: `include/uapi/asm-generic/fcntl.h` on x86-64, `arch/arm64/include/uapi/asm/fcntl.h`
/// on arm64.
#[cfg(target_arch = "x86_64")]
const O_DIRECT: u32 = 0o40_000;
#[cfg(target_arch = "aarch64")]
const O_DIRECT: u32 = 0o200_000;

/// The flags of this process's open descriptor of `path`: the octal `flags:` line of `/proc/self/fdinfo/<fd>`, for
/// the descriptor whose link `/proc/self/fd/<fd>` names `path` (proc(5)).
fn open_flags(path: &Path) -> u32 {
    let path = std::fs::canonicalize(path).expect("the file exists");
    let fd = std::fs::read_dir("/proc/self/fd")
        .expect("procfs")
        .filter_map(Result::ok)
        .find(|entry| std::fs::read_link(entry.path()).is_ok_and(|target| target == path))
        .expect("the file is open")
        .file_name();
    let info = std::fs::read_to_string(Path::new("/proc/self/fdinfo").join(fd)).expect("fdinfo");
    let flags = info
        .lines()
        .find_map(|line| line.strip_prefix("flags:"))
        .expect("a flags line")
        .trim();
    u32::from_str_radix(flags, 8).expect("octal flags")
}

#[test]
fn test_an_aligned_file_is_opened_with_o_direct_and_a_plain_one_without() {
    let tmp = TempDir::new("odirect");
    let (aligned, plain) = (tmp.path().join("a"), tmp.path().join("p"));
    run(async {
        let a = CompioStorage
            .open_aligned(&aligned, OpenMode::Create)
            .await
            .expect("created");
        let p = CompioStorage
            .open(&plain, OpenMode::Create)
            .await
            .expect("created");
        assert_ne!(open_flags(&aligned) & O_DIRECT, 0, "the aligned file");
        assert_eq!(open_flags(&plain) & O_DIRECT, 0, "the plain file");
        // Reopened for reading, as the log reads its segments when it opens.
        let r = CompioStorage
            .open_aligned(&aligned, OpenMode::Read)
            .await
            .expect("opened");
        drop((a, p));
        assert_ne!(
            open_flags(&aligned) & O_DIRECT,
            0,
            "the aligned file reopened"
        );
        drop(r);
    });
}
