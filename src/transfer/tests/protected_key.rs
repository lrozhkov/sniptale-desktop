use super::super::{
    error::TransferError,
    protected_file::PROTECTED_MEDIA_ALGORITHM,
    protected_key::{
        MediaKeyProtection, ProtectedMediaKey, ProtectedMediaKeyError, new_protected_media_key,
        unprotect_media_key, validate_protected_media_key,
    },
};
use proptest::prelude::*;

#[test]
fn protected_media_key_rejects_wrong_algorithm_and_invalid_metadata() -> Result<(), TransferError> {
    let (mut protection, _) = new_protected_media_key(PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| TransferError::ProtectedMedia)?;

    protection.algorithm = "xchacha20poly1305-unknown".to_owned();
    assert_eq!(
        validate_protected_media_key(&protection, PROTECTED_MEDIA_ALGORITHM),
        Err(ProtectedMediaKeyError::UnsupportedProtection)
    );
    assert_eq!(
        unprotect_media_key(&protection, PROTECTED_MEDIA_ALGORITHM),
        Err(ProtectedMediaKeyError::UnsupportedProtection)
    );

    protection.algorithm = PROTECTED_MEDIA_ALGORITHM.to_owned();
    protection.protected_key_hex.clear();
    assert_eq!(
        validate_protected_media_key(&protection, PROTECTED_MEDIA_ALGORITHM),
        Err(ProtectedMediaKeyError::InvalidKey)
    );

    protection.protected_key_hex = "ab".repeat(32);
    protection.nonce_prefix_hex = "ab".repeat(15);
    assert_eq!(
        validate_protected_media_key(&protection, PROTECTED_MEDIA_ALGORITHM),
        Err(ProtectedMediaKeyError::InvalidNonce)
    );
    Ok(())
}

proptest! {
    #[test]
    fn protected_media_key_rejects_malformed_hex_metadata(invalid in "[g-z! -]{1,8}") {
        let protection = ProtectedMediaKey {
            algorithm: PROTECTED_MEDIA_ALGORITHM.to_owned(),
            key_protection: MediaKeyProtection::EquivalentCurrentUser,
            protected_key_hex: invalid.clone(),
            nonce_prefix_hex: "ab".repeat(16),
        };
        prop_assert_eq!(
            validate_protected_media_key(&protection, PROTECTED_MEDIA_ALGORITHM),
            Err(ProtectedMediaKeyError::InvalidHex)
        );

        let protection = ProtectedMediaKey {
            protected_key_hex: "ab".repeat(32),
            nonce_prefix_hex: invalid,
            ..protection
        };
        prop_assert_eq!(
            validate_protected_media_key(&protection, PROTECTED_MEDIA_ALGORITHM),
            Err(ProtectedMediaKeyError::InvalidHex)
        );
    }
}

#[cfg(not(windows))]
#[test]
fn non_windows_media_key_mapping_rejects_current_user_dpapi_metadata() {
    let protection = ProtectedMediaKey {
        algorithm: PROTECTED_MEDIA_ALGORITHM.to_owned(),
        key_protection: MediaKeyProtection::CurrentUserDpapi,
        protected_key_hex: "ab".repeat(32),
        nonce_prefix_hex: "cd".repeat(16),
    };

    assert_eq!(
        unprotect_media_key(&protection, PROTECTED_MEDIA_ALGORITHM),
        Err(ProtectedMediaKeyError::UnsupportedProtection)
    );
}

#[cfg(not(windows))]
#[test]
fn non_windows_equivalent_current_user_media_key_round_trips() -> Result<(), ProtectedMediaKeyError>
{
    use super::super::protected_key::MediaKeyProtection;

    let (protection, key) = new_protected_media_key(PROTECTED_MEDIA_ALGORITHM)?;
    assert_eq!(
        protection.key_protection,
        MediaKeyProtection::EquivalentCurrentUser
    );

    let (unwrapped, nonce_prefix) = unprotect_media_key(&protection, PROTECTED_MEDIA_ALGORITHM)?;

    assert_eq!(unwrapped, key);
    assert_eq!(nonce_prefix.len(), 16);
    Ok(())
}
