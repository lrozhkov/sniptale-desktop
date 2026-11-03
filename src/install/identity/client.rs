//! Public, installation-bound identity derived without exposing authentication material.

use sha2::{Digest, Sha256};

/// Separating the domain and channel prevents reuse as a pipe authentication credential.
pub(super) fn derive_client_id(install_id: &str, secret: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"sniptale-desktop-client-v1\0");
    digest.update(install_id.as_bytes());
    digest.update(b"\0");
    digest.update(secret);
    format!("{:x}", digest.finalize())
}

#[cfg(all(windows, not(test)))]
pub(crate) fn current_desktop_client_id()
-> Result<String, super::super::pipe_secret_store::PipeSecretLoadError> {
    let identity = super::expected_install_identity();
    let secret =
        super::super::pipe_secret_store::load_current_user_pipe_secret(identity.install_id)?;
    let client_id = derive_client_id(identity.install_id, secret.secret_material.as_bytes());
    // This owner does not retain an extra copy of the unwrapped credential.
    let mut material = secret.secret_material.into_bytes();
    material.fill(0);
    Ok(client_id)
}

#[cfg(test)]
mod tests {
    use super::derive_client_id;

    #[test]
    fn client_identity_survives_restart_and_changes_after_reinstall_or_channel_change() {
        let first = derive_client_id("install-dev", b"first-install-secret");
        assert_eq!(
            first,
            derive_client_id("install-dev", b"first-install-secret")
        );
        assert_ne!(
            first,
            derive_client_id("install-dev", b"second-install-secret")
        );
        assert_ne!(
            first,
            derive_client_id("install-stable", b"first-install-secret")
        );
        assert_eq!(first.len(), 64);
        assert!(
            first
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        assert!(!first.contains("first-install-secret"));
    }
}
