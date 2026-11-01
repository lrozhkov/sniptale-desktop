//! Safe Windows filesystem metadata projection; no raw handles cross this owner-local adapter.
#![cfg(windows)]

use std::os::windows::fs::MetadataExt;

pub(super) fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_attributes() & 0x400 != 0
}
