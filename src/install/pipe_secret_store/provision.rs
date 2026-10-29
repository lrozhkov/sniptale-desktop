//! First-resident-launch publication of a current-user-protected pipe credential.

use std::{
    fs,
    io::{self, Write},
    path::Path,
};

use serde::Serialize;

use crate::install::pipe_secret::{
    PIPE_SECRET_AUTH_ALGORITHM, PipeSecretRecord, SecretProtection, validate_pipe_secret_record,
};

#[derive(Debug)]
pub(crate) enum PipeSecretProvisionError {
    #[cfg(windows)]
    Io,
    InvalidRecord,
    #[cfg(windows)]
    Protection,
    #[cfg(windows)]
    Randomness,
    #[cfg(windows)]
    User,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PipeSecretOutput<'a> {
    install_id: &'a str,
    current_user_sid: &'a str,
    auth_algorithm: &'static str,
    protection: &'static str,
    protected_secret_hex: String,
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                char::from(HEX[usize::from(byte >> 4)]),
                char::from(HEX[usize::from(byte & 15)]),
            ]
        })
        .collect()
}

fn protected_payload(
    install_id: &str,
    sid: &str,
    protected: &[u8],
) -> Result<Vec<u8>, PipeSecretProvisionError> {
    validate_pipe_secret_record(
        &PipeSecretRecord {
            install_id: install_id.to_owned(),
            current_user_sid: sid.to_owned(),
            auth_algorithm: PIPE_SECRET_AUTH_ALGORITHM.to_owned(),
            protection: SecretProtection::CurrentUserDpapi,
            protected_secret_bytes: protected.len(),
        },
        install_id,
        sid,
    )
    .map_err(|_| PipeSecretProvisionError::InvalidRecord)?;
    serde_json::to_vec(&PipeSecretOutput {
        install_id,
        current_user_sid: sid,
        auth_algorithm: PIPE_SECRET_AUTH_ALGORITHM,
        protection: "current-user-dpapi",
        protected_secret_hex: encode_hex(protected),
    })
    .map_err(|_| PipeSecretProvisionError::InvalidRecord)
}

// Hard-link publication exposes only a complete synced ciphertext and never replaces an existing
// record. Concurrent first launches converge on the winner; readers still validate that record.
fn publish_if_missing(path: &Path, temporary: &Path, payload: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    let result = (|| {
        file.write_all(payload)?;
        file.sync_all()?;
        drop(file);
        match fs::hard_link(temporary, path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(error),
        }
    })();
    let cleanup = fs::remove_file(temporary);
    result.and(cleanup)
}

#[cfg(windows)]
pub(crate) fn initialize_current_user_pipe_secret(
    install_id: &str,
) -> Result<(), PipeSecretProvisionError> {
    use crate::install::{
        pipe_secret_store::pipe_secret_file_path,
        windows::{dpapi::protect_current_user_pipe_secret, user::current_user_sid},
    };
    let root = std::env::var_os("LOCALAPPDATA").ok_or(PipeSecretProvisionError::Io)?;
    let path = pipe_secret_file_path(Path::new(&root));
    match fs::symlink_metadata(&path) {
        Ok(_) => return Ok(()), // Existing malformed state must fail in the loader, never rotate here.
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return Err(PipeSecretProvisionError::Io),
    }
    let sid = current_user_sid().map_err(|_| PipeSecretProvisionError::User)?;
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|_| PipeSecretProvisionError::Randomness)?;
    let mut secret = encode_hex(&random).into_bytes();
    random.fill(0);
    let protected = protect_current_user_pipe_secret(&secret);
    secret.fill(0);
    let protected = protected.map_err(|_| PipeSecretProvisionError::Protection)?;
    let payload = protected_payload(install_id, &sid, &protected)?;
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| PipeSecretProvisionError::Randomness)?;
    let temporary = path.with_file_name(format!(".pipe-secret-{}.tmp", encode_hex(&nonce)));
    let parent = path.parent().ok_or(PipeSecretProvisionError::Io)?;
    fs::create_dir_all(parent).map_err(|_| PipeSecretProvisionError::Io)?;
    publish_if_missing(&path, &temporary, &payload).map_err(|_| PipeSecretProvisionError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::pipe_secret_store::parse_pipe_secret_file;

    #[test]
    fn first_publication_is_parseable_and_concurrent_publication_cannot_replace_it()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("pipe-secret.json");
        let payload = protected_payload("sniptale-dev-install", "S-1-5-21-123", &[7; 64])
            .map_err(|_| io::Error::other("fixture rejected"))?;
        let temporary = directory.path().join("first.tmp");
        publish_if_missing(&path, &temporary, &payload)?;
        let parsed =
            parse_pipe_secret_file(&fs::read(&path)?, "sniptale-dev-install", "S-1-5-21-123")?;
        assert_eq!(parsed.protected_secret, vec![7; 64]);
        assert!(!temporary.exists());
        publish_if_missing(&path, &temporary, b"replacement must not win")?;
        assert_eq!(fs::read(&path)?, payload);
        assert!(!temporary.exists());
        fs::write(&path, b"corrupt existing state")?;
        publish_if_missing(&path, &temporary, &payload)?;
        assert_eq!(fs::read(&path)?, b"corrupt existing state");
        assert!(
            parse_pipe_secret_file(&fs::read(&path)?, "sniptale-dev-install", "S-1-5-21-123")
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn provisioning_rejects_invalid_identity_or_protection_and_preserves_foreign_temporary_file()
    -> Result<(), Box<dyn std::error::Error>> {
        for (id, sid, blob) in [
            ("bad", "S-1-5-21-123", vec![7; 64]),
            ("sniptale-dev-install", "wrong", vec![7; 64]),
            ("sniptale-dev-install", "S-1-5-21-123", vec![7; 3]),
        ] {
            assert!(protected_payload(id, sid, &blob).is_err());
        }
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("pipe-secret.json");
        let temporary = directory.path().join("occupied.tmp");
        fs::write(&temporary, b"foreign")?;
        assert!(publish_if_missing(&path, &temporary, b"new").is_err());
        assert!(!path.exists());
        assert_eq!(fs::read(&temporary)?, b"foreign");
        let missing = directory.path().join("missing").join("pipe-secret.json");
        let unused = directory.path().join("unused.tmp");
        assert!(publish_if_missing(&missing, &unused, b"new").is_err());
        assert!(!unused.exists());
        Ok(())
    }

    #[test]
    fn provisioned_record_rejects_wrong_owner_and_oversized_input()
    -> Result<(), Box<dyn std::error::Error>> {
        let payload = protected_payload("sniptale-dev-install", "S-1-5-21-123", &[7; 64])
            .map_err(|_| io::Error::other("fixture rejected"))?;
        assert!(
            parse_pipe_secret_file(&payload, "sniptale-other-install", "S-1-5-21-123").is_err()
        );
        assert!(parse_pipe_secret_file(&payload, "sniptale-dev-install", "S-1-5-21-456").is_err());
        assert!(
            parse_pipe_secret_file(
                &vec![b' '; 16 * 1024 + 1],
                "sniptale-dev-install",
                "S-1-5-21-123"
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn simultaneous_first_launches_publish_one_complete_credential()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("pipe-secret.json");
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| -> io::Result<()> {
            let mut handles = Vec::new();
            for index in 0..2 {
                let path = &path;
                let barrier = &barrier;
                let temporary = directory.path().join(format!("launch-{index}.tmp"));
                handles.push(scope.spawn(move || {
                    barrier.wait();
                    publish_if_missing(
                        path,
                        &temporary,
                        if index == 0 {
                            b"first-complete"
                        } else {
                            b"second-complete"
                        },
                    )
                }));
            }
            for handle in handles {
                handle
                    .join()
                    .map_err(|_| io::Error::other("publication thread failed"))??;
            }
            Ok(())
        })?;
        let actual = fs::read(&path)?;
        assert!(actual == b"first-complete" || actual == b"second-complete");
        assert_eq!(fs::read_dir(directory.path())?.count(), 1);
        Ok(())
    }
}
