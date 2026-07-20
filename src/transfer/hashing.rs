//! SHA-256 whole-file and per-chunk hashing owner.

use sha2::{Digest, Sha256};

#[derive(Clone, Debug)]
pub(crate) struct Sha256Accumulator {
    hasher: Sha256,
}

impl Sha256Accumulator {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }

    #[must_use]
    pub(crate) fn finalize_hex(self) -> String {
        digest_to_hex(self.hasher.finalize())
    }
}

#[must_use]
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    digest_to_hex(Sha256::digest(bytes))
}

fn digest_to_hex(digest: impl IntoIterator<Item = u8>) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest {
        output.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        output.push(char::from(b"0123456789abcdef"[usize::from(byte & 0x0f)]));
    }
    output
}
