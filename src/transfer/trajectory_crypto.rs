//! Per-trajectory protection; plaintext blocks never need a staging file.

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::fmt;

use super::protected_key::{ProtectedMediaKey, new_protected_media_key, unprotect_media_key};

pub(crate) const TRAJECTORY_PROTECTION: &str = "cursor-trajectory-xchacha20poly1305-block-v1";

#[derive(Clone, Copy, Debug)]
pub(super) enum HistoryProtection {
    Cursor,
    Events,
}

impl HistoryProtection {
    const fn label(self) -> &'static str {
        match self {
            Self::Cursor => TRAJECTORY_PROTECTION,
            Self::Events => "recording-events-xchacha20poly1305-block-v1",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrajectoryCryptoError {
    Key,
    Random,
    InvalidBlock,
    Authentication,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum TrajectoryAuthentication {
    Ledger,
    Index,
}

pub(super) struct TrajectoryCipher {
    key: [u8; 32],
    mac_key: [u8; 32],
    binding: Vec<u8>,
}

impl fmt::Debug for TrajectoryCipher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrajectoryCipher")
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(super) struct SealedTrajectoryBlock {
    pub(super) nonce: [u8; 24],
    pub(super) ciphertext: Vec<u8>,
}

impl TrajectoryCipher {
    pub(super) fn create(
        recording_id: &str,
        trajectory_id: &str,
    ) -> Result<(Self, ProtectedMediaKey), TrajectoryCryptoError> {
        Self::create_for(recording_id, trajectory_id, HistoryProtection::Cursor)
    }

    pub(super) fn create_for(
        recording_id: &str,
        stream_id: &str,
        protection: HistoryProtection,
    ) -> Result<(Self, ProtectedMediaKey), TrajectoryCryptoError> {
        let (protected, mut initial_key) =
            new_protected_media_key(protection.label()).map_err(|_| TrajectoryCryptoError::Key)?;
        let opened = Self::open_for(recording_id, stream_id, &protected, protection);
        initial_key.fill(0);
        opened.map(|cipher| (cipher, protected))
    }

    pub(super) fn open(
        recording_id: &str,
        trajectory_id: &str,
        protected: &ProtectedMediaKey,
    ) -> Result<Self, TrajectoryCryptoError> {
        Self::open_for(
            recording_id,
            trajectory_id,
            protected,
            HistoryProtection::Cursor,
        )
    }

    pub(super) fn open_for(
        recording_id: &str,
        stream_id: &str,
        protected: &ProtectedMediaKey,
        protection: HistoryProtection,
    ) -> Result<Self, TrajectoryCryptoError> {
        let (key, prefix) = unprotect_media_key(protected, protection.label())
            .map_err(|_| TrajectoryCryptoError::Key)?;
        let mut cipher = Self {
            key,
            mac_key: [0; 32],
            binding: Vec::new(),
        };
        let length = u32::try_from(recording_id.len()).map_err(|_| TrajectoryCryptoError::Key)?;
        cipher.binding.extend_from_slice(&length.to_le_bytes());
        cipher.binding.extend_from_slice(recording_id.as_bytes());
        cipher.binding.extend_from_slice(stream_id.as_bytes());
        cipher
            .binding
            .extend_from_slice(protection.label().as_bytes());
        cipher.binding.extend_from_slice(&prefix);
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&cipher.key)
            .map_err(|_| TrajectoryCryptoError::Key)?;
        mac.update(b"sniptale-cursor-trajectory-auth-key-v1\0");
        cipher.mac_key.copy_from_slice(&mac.finalize().into_bytes());
        Ok(cipher)
    }

    pub(super) fn seal(
        &self,
        sequence: u64,
        plaintext: &[u8],
    ) -> Result<SealedTrajectoryBlock, TrajectoryCryptoError> {
        if plaintext.is_empty() || plaintext.len() > 65536 {
            return Err(TrajectoryCryptoError::InvalidBlock);
        }
        let mut nonce = [0; 24];
        getrandom::fill(&mut nonce).map_err(|_| TrajectoryCryptoError::Random)?;
        let cipher =
            XChaCha20Poly1305::new_from_slice(&self.key).map_err(|_| TrajectoryCryptoError::Key)?;
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: &self.block_aad(sequence, plaintext.len())?,
                },
            )
            .map_err(|_| TrajectoryCryptoError::Authentication)?;
        Ok(SealedTrajectoryBlock { nonce, ciphertext })
    }

    pub(super) fn unseal(
        &self,
        sequence: u64,
        plaintext_bytes: usize,
        nonce: &[u8; 24],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, TrajectoryCryptoError> {
        if plaintext_bytes == 0
            || plaintext_bytes > 65536
            || ciphertext.len() != plaintext_bytes + 16
        {
            return Err(TrajectoryCryptoError::InvalidBlock);
        }
        XChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|_| TrajectoryCryptoError::Key)?
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: &self.block_aad(sequence, plaintext_bytes)?,
                },
            )
            .map_err(|_| TrajectoryCryptoError::Authentication)
    }

    pub(super) fn authenticate(
        &self,
        kind: TrajectoryAuthentication,
        bytes: &[u8],
    ) -> Result<[u8; 32], TrajectoryCryptoError> {
        let mut mac = self.authenticator(kind)?;
        mac.update(bytes);
        Ok(mac.finalize().into_bytes().into())
    }

    pub(super) fn verify(
        &self,
        kind: TrajectoryAuthentication,
        bytes: &[u8],
        tag: &[u8],
    ) -> Result<(), TrajectoryCryptoError> {
        let mut mac = self.authenticator(kind)?;
        mac.update(bytes);
        mac.verify_slice(tag)
            .map_err(|_| TrajectoryCryptoError::Authentication)
    }

    fn authenticator(
        &self,
        kind: TrajectoryAuthentication,
    ) -> Result<Hmac<Sha256>, TrajectoryCryptoError> {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.mac_key)
            .map_err(|_| TrajectoryCryptoError::Key)?;
        mac.update(b"sniptale-cursor-trajectory-auth-v1\0");
        mac.update(match kind {
            TrajectoryAuthentication::Ledger => b"ledger\0",
            TrajectoryAuthentication::Index => b"index\0",
        });
        mac.update(&self.binding);
        Ok(mac)
    }

    fn block_aad(
        &self,
        sequence: u64,
        plaintext_bytes: usize,
    ) -> Result<Vec<u8>, TrajectoryCryptoError> {
        let mut aad = b"sniptale-cursor-trajectory-block-v1\0".to_vec();
        aad.extend_from_slice(&self.binding);
        aad.extend_from_slice(&sequence.to_le_bytes());
        aad.extend_from_slice(
            &u32::try_from(plaintext_bytes)
                .map_err(|_| TrajectoryCryptoError::InvalidBlock)?
                .to_le_bytes(),
        );
        Ok(aad)
    }
}

impl Drop for TrajectoryCipher {
    fn drop(&mut self) {
        self.key.fill(0);
        self.mac_key.fill(0);
    }
}
