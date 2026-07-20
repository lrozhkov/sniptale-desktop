use super::super::identity::{
    TransferIdentityError, TransferMediaFileKind, derived_media_id, media_filename, media_path,
    validate_media_filename, validate_media_id, validate_sha256_hex,
};
use proptest::prelude::*;

fn ascii_fragment() -> impl Strategy<Value = String> {
    prop::collection::vec(b'a'..=b'z', 0..16)
        .prop_map(|bytes| bytes.into_iter().map(char::from).collect())
}

fn pathlike_media_id() -> impl Strategy<Value = String> {
    (
        ascii_fragment(),
        prop_oneof![Just('/'), Just('\\'), Just(':'), Just('\n')],
        ascii_fragment(),
    )
        .prop_map(|(prefix, separator, suffix)| format!("{prefix}{separator}{suffix}"))
}

#[test]
fn transfer_identity_derives_media_ids_without_tightening_protocol_ids() {
    let recording_id = derived_media_id("recording", "command:with.protocol-compatible:colon");

    assert!(recording_id.starts_with("recording-"));
    assert_eq!(recording_id.len(), "recording-".len() + 64);
    assert_eq!(validate_media_id(&recording_id), Ok(()));
    assert!(!recording_id.contains(':'));
}

#[test]
fn transfer_identity_rejects_path_and_device_name_media_ids() {
    for media_id in [
        "",
        "../capture",
        "capture/one",
        "capture\\one",
        "capture:ads",
        "C:capture",
        ".capture",
        "capture.",
        "capture..one",
        "con",
        "capture.con",
        "lpt1",
        "capture\none",
        "capture-é",
    ] {
        assert!(
            validate_media_id(media_id).is_err(),
            "media id should be rejected: {media_id:?}"
        );
    }
}

proptest! {
    #[test]
    fn transfer_identity_rejects_pathlike_or_reserved_filename_fragments(
        media_id in pathlike_media_id()
    ) {
        prop_assert_eq!(validate_media_id(&media_id), Err(TransferIdentityError::InvalidId));
        let filename = format!("{media_id}.png");
        prop_assert_eq!(
            validate_media_filename(&filename, TransferMediaFileKind::Png),
            Err(TransferIdentityError::InvalidFilename)
        );
    }
}

#[test]
fn transfer_identity_builds_safe_leaf_paths_and_validates_filenames()
-> Result<(), TransferIdentityError> {
    let root = std::path::Path::new("root");
    let capture_id = derived_media_id("capture", "command-1");
    let screenshot = media_filename(&capture_id, TransferMediaFileKind::Png)?;
    let screenshot_path = media_path(root, &capture_id, TransferMediaFileKind::Png)?;

    assert_eq!(screenshot, format!("{capture_id}.png"));
    assert_eq!(screenshot_path, root.join(&screenshot));
    assert_eq!(
        validate_media_filename(&screenshot, TransferMediaFileKind::Png),
        Ok(())
    );
    assert_eq!(
        validate_media_filename("../capture.png", TransferMediaFileKind::Png),
        Err(TransferIdentityError::InvalidFilename)
    );
    assert_eq!(
        validate_media_filename("con.mp4", TransferMediaFileKind::Mp4),
        Err(TransferIdentityError::InvalidFilename)
    );
    assert_eq!(
        validate_media_filename("recording.MP4", TransferMediaFileKind::Mp4),
        Err(TransferIdentityError::InvalidFilename)
    );
    Ok(())
}

#[test]
fn transfer_identity_validates_lowercase_sha256_hex() {
    assert_eq!(
        validate_sha256_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        Ok(())
    );
    assert_eq!(
        validate_sha256_hex("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        Err(TransferIdentityError::InvalidHash)
    );
    assert_eq!(
        validate_sha256_hex("not-a-sha"),
        Err(TransferIdentityError::InvalidHash)
    );
}
