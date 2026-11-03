use super::*;

fn target(profile: &str, connection: &str) -> Result<BrowserTarget, BrowserTargetError> {
    BrowserTarget::from_controller(&NativeControllerIdentity {
        extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        browser_family: BrowserFamily::Chrome,
        profile_key: profile.to_owned(),
        connection_id: connection.to_owned(),
        document_id: Some(connection.to_owned()),
    })
}

#[test]
fn selection_is_explicit_persistent_and_ignores_runtime_connection_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("targets.json");
    let mut registry = BrowserTargetRegistry::load(path.clone()).map_err(|_| "load")?;
    let first = target("profile-one", "connection-one").map_err(|_| "target")?;
    assert_eq!(
        first,
        target("profile-one", "restarted").map_err(|_| "target")?
    );
    registry.register(first.clone()).map_err(|_| "register")?;
    registry
        .register(target("profile-two", "two").map_err(|_| "target")?)
        .map_err(|_| "register")?;
    assert_eq!(registry.default_target_id(), None);
    registry
        .select(Some(&first.id), true)
        .map_err(|_| "select")?;
    let reloaded = BrowserTargetRegistry::load(path).map_err(|_| "reload")?;
    assert_eq!(reloaded.default_target_id(), Some(first.id.as_str()));
    assert!(reloaded.ask_after_capture());
    assert_eq!(reloaded.targets().len(), 2);
    assert_eq!(
        registry.select(Some("unknown"), false),
        Err(BrowserTargetError::Stale)
    );
    assert_eq!(registry.default_target_id(), Some(first.id.as_str()));
    Ok(())
}

#[test]
fn invalid_oversized_duplicate_or_dangling_registry_fails_closed_without_repair()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("targets.json");
    for payload in [
        b"not-json".to_vec(),
        vec![b' '; 32 * 1024 + 1],
        br#"{"schemaVersion":1,"targets":[],"defaultTargetId":"absent","askAfterCapture":false}"#
            .to_vec(),
    ] {
        std::fs::write(&path, &payload)?;
        assert!(BrowserTargetRegistry::load(path.clone()).is_err());
        assert_eq!(std::fs::read(&path)?, payload);
    }
    let first = target("profile", "one").map_err(|_| "target")?;
    let state = BrowserTargetState {
        targets: vec![first.clone(), first],
        ..BrowserTargetState::default()
    };
    assert_eq!(validate(&state), Err(BrowserTargetError::Invalid));
    assert_eq!(
        target("path/unsafe", "one"),
        Err(BrowserTargetError::Invalid)
    );
    Ok(())
}

#[test]
fn delivery_activation_is_persistent_and_a_new_default_resets_the_override()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("targets.json");
    let active = crate::settings::ActiveSettings::extension_managed_default()
        .with_browser_target_store(path.clone())
        .map_err(|_| "registry")?;
    let controller = NativeControllerIdentity {
        extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        browser_family: BrowserFamily::Edge,
        profile_key: "profile-one".to_owned(),
        connection_id: "connection-one".to_owned(),
        document_id: None,
    };
    let browser = active
        .register_browser_target(&controller)
        .map_err(|_| "register")?;
    active
        .activate_browser_delivery(&browser.id)
        .map_err(|_| "activate")?;
    let recovered = BrowserTargetRegistry::load(path).map_err(|_| "load")?;
    assert_eq!(recovered.default_target_id(), None);
    assert_eq!(recovered.selected_target_id(), Some(browser.id.as_str()));
    assert_eq!(browser.label(), "Edge — profile-one");
    assert_eq!(
        active.activate_browser_delivery("unknown"),
        Err(BrowserTargetError::Stale)
    );
    active
        .select_browser_target(None, true)
        .map_err(|_| "reset")?;
    assert_eq!(
        active
            .browser_targets()
            .map_err(|_| "snapshot")?
            .selected_target_id(),
        None
    );
    Ok(())
}

#[test]
fn full_invalid_and_duplicate_registrations_do_not_change_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let mut registry = BrowserTargetRegistry::default();
    for index in 0..MAX_TARGETS {
        registry
            .register(target(&format!("profile-{index}"), "one").map_err(|_| "target")?)
            .map_err(|_| "register")?;
    }
    let first = registry.targets()[0].clone();
    registry
        .select(Some(&first.id), false)
        .map_err(|_| "select")?;
    registry.register(first.clone()).map_err(|_| "idempotent")?;
    assert_eq!(
        registry.register(target("overflow", "one").map_err(|_| "target")?),
        Err(BrowserTargetError::Limit)
    );
    assert_eq!(registry.default_target_id(), Some(first.id.as_str()));
    for profile in ["", "path/unsafe", &"x".repeat(129)] {
        assert_eq!(target(profile, "one"), Err(BrowserTargetError::Invalid));
    }
    for family in [BrowserFamily::Chromium, BrowserFamily::Unknown] {
        let browser = BrowserTarget::from_controller(&NativeControllerIdentity {
            extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            browser_family: family,
            profile_key: "profile".to_owned(),
            document_id: None,
            connection_id: "one".to_owned(),
        })
        .map_err(|_| "target")?;
        assert!(browser.label().ends_with(" — profile"));
    }
    Ok(())
}

#[test]
fn concurrent_writer_pending_file_is_never_removed_or_adopted()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("targets.json");
    let mut registry = BrowserTargetRegistry::load(path.clone()).map_err(|_| "load registry")?;
    let first = target("profile-one", "one").map_err(|_| "target identity")?;
    registry
        .register(first.clone())
        .map_err(|_| "register target")?;
    let committed = std::fs::read(&path)?;
    let pending = path.with_extension("pending");
    std::fs::write(&pending, b"other writer owns this file")?;
    assert_eq!(
        registry.select(Some(&first.id), true),
        Err(BrowserTargetError::Io)
    );
    assert_eq!(registry.default_target_id(), None);
    assert_eq!(std::fs::read(&pending)?, b"other writer owns this file");
    assert_eq!(std::fs::read(&path)?, committed);
    Ok(())
}
