//! Executable identity proof for resident/native-host copies.

use std::{io::Read, path::Path};

pub(crate) fn accepted_executable_identity(
    observed: &Path,
    resident: &Path,
    registered: Option<&Path>,
) -> Option<String> {
    let observed_hash = file_sha256_hex(observed)?;
    let same_resident_path = same_windows_path(observed, resident);
    let same_resident_bytes = file_sha256_hex(resident).as_deref() == Some(&observed_hash);
    let registered_path = registered.is_some_and(|path| same_windows_path(observed, path));
    (same_resident_path || same_resident_bytes || registered_path).then_some(observed_hash)
}

fn file_sha256_hex(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = crate::transfer::hashing::Sha256Accumulator::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(hasher.finalize_hex())
}

fn same_windows_path(left: &Path, right: &Path) -> bool {
    normalized_windows_path(left) == normalized_windows_path(right)
}

fn normalized_windows_path(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::accepted_executable_identity;
    use std::{fs, io};

    #[test]
    fn accepts_same_path_and_returns_its_hash() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let executable = directory.path().join("agent.exe");
        fs::write(&executable, b"sniptale-agent")?;

        let hash = accepted_executable_identity(&executable, &executable, None)
            .ok_or_else(|| io::Error::other("same path rejected"))?;

        assert_eq!(
            hash,
            crate::transfer::hashing::sha256_hex(b"sniptale-agent")
        );
        Ok(())
    }

    #[test]
    fn accepts_identical_copies_but_rejects_different_or_missing_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let resident = directory.path().join("resident.exe");
        let copy = directory.path().join("registered.exe");
        let different = directory.path().join("different.exe");
        fs::write(&resident, b"same-binary")?;
        fs::write(&copy, b"same-binary")?;
        fs::write(&different, b"different-binary")?;

        assert!(accepted_executable_identity(&copy, &resident, None).is_some());
        assert!(accepted_executable_identity(&different, &resident, None).is_none());
        assert!(
            accepted_executable_identity(&directory.path().join("missing.exe"), &resident, None)
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn accepts_the_validated_registered_path_even_when_resident_bytes_differ()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let resident = directory.path().join("dev-resident.exe");
        let registered = directory.path().join("installed-host.exe");
        fs::write(&resident, b"new-development-build")?;
        fs::write(&registered, b"installed-build")?;

        assert!(accepted_executable_identity(&registered, &resident, Some(&registered)).is_some());
        assert!(accepted_executable_identity(&resident, &registered, None).is_none());
        Ok(())
    }
}
