use super::*;

#[test]
fn protocol_constants_match_tracked_contract_index() -> Result<(), Box<dyn Error>> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("contracts/native/contract-index.json");
    let index: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    assert_eq!(index["protocolVersion"], u64::from(PROTOCOL_VERSION));
    assert_eq!(
        index["settingsSchemaVersion"],
        u64::from(SETTINGS_SCHEMA_VERSION)
    );
    assert_eq!(index["cacheSchemaVersion"], u64::from(CACHE_SCHEMA_VERSION));
    assert_eq!(
        index["nativeMessagingLimits"]["hostToExtensionMaxMessageBytes"],
        HOST_TO_EXTENSION_MAX_MESSAGE_BYTES
    );
    assert_eq!(
        index["nativeMessagingLimits"]["extensionToHostMaxMessageBytes"],
        EXTENSION_TO_HOST_MAX_MESSAGE_BYTES
    );
    assert_eq!(
        index["nativeMessagingLimits"]["extensionToHostControlMaxMessageBytes"],
        EXTENSION_TO_HOST_CONTROL_MAX_MESSAGE_BYTES
    );
    assert_eq!(
        index["nativeMessagingLimits"]["extensionToHostChromiumMaxMessageBytes"],
        EXTENSION_TO_HOST_CHROMIUM_MAX_MESSAGE_BYTES
    );
    assert_eq!(
        index["nativeMessagingLimits"]["targetJsonUtf8Bytes"],
        MESSAGE_TARGET_MAX_JSON_UTF8_BYTES
    );
    assert_eq!(
        index["nativeMessagingLimits"]["chunkRawBytes"],
        CHUNK_RAW_BYTES
    );
    assert_eq!(
        index["nativeMessagingLimits"]["chunkMaxBase64Chars"],
        CHUNK_MAX_BASE64_CHARS
    );
    assert_eq!(
        index["nativeMessagingLimits"]["screenshotMaxTotalBytes"],
        SCREENSHOT_MAX_TOTAL_BYTES
    );
    assert_eq!(
        index["nativeMessagingLimits"]["recordingMaxTotalBytes"],
        RECORDING_MAX_TOTAL_BYTES
    );

    let fixtures = load_fixtures("positive")?;
    let extension_hello = &fixtures["ExtensionHelloMessage"];
    let app_hello = &fixtures["AppHelloMessage"];
    assert_eq!(extension_hello["minAppVersion"], MIN_SUPPORTED_APP_VERSION);
    assert_eq!(
        app_hello["minExtensionVersion"],
        MIN_SUPPORTED_EXTENSION_VERSION
    );
    Ok(())
}

#[test]
fn operation_error_catalog_matches_protocol_taxonomy() -> Result<(), Box<dyn Error>> {
    let catalog: Value = serde_json::from_str(&fs::read_to_string(error_catalog_path())?)?;
    assert_eq!(catalog["schemaVersion"], 1);

    let public_codes = catalog["publicCodes"]
        .as_array()
        .ok_or("publicCodes must be an array")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or("publicCodes entries must be strings")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let expected_public_codes = NativeAppErrorCode::ALL
        .iter()
        .copied()
        .map(serialized_error_code)
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(public_codes, expected_public_codes);

    let public_code_set = public_codes.iter().cloned().collect::<BTreeSet<_>>();
    let operation_failures = catalog["operationFailures"]
        .as_object()
        .ok_or("operationFailures must be an object")?;
    let expected_kinds = OperationalErrorKind::ALL
        .iter()
        .map(|kind| kind.descriptor().catalog_key.to_owned())
        .collect::<BTreeSet<_>>();
    let actual_kinds = operation_failures.keys().cloned().collect::<BTreeSet<_>>();
    assert_eq!(actual_kinds, expected_kinds);

    for kind in OperationalErrorKind::ALL {
        let descriptor = kind.descriptor();
        let entry = operation_failures
            .get(descriptor.catalog_key)
            .ok_or("missing operation error catalog entry")?;
        assert_eq!(
            entry["publicCode"],
            serialized_error_code(descriptor.public_code)?
        );
        assert_eq!(entry["recoverable"], descriptor.recoverable);
        assert!(
            public_code_set.contains(
                entry["publicCode"]
                    .as_str()
                    .ok_or("operation publicCode must be a string")?
            )
        );
        let default_phase = entry["defaultPhase"]
            .as_str()
            .ok_or("operation defaultPhase must be a string")?;
        assert_ne!(default_phase.len(), 0);
        assert!(!default_phase.contains('/'));
    }
    Ok(())
}
