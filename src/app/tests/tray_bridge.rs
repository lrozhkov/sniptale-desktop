use super::*;

#[test]
fn legacy_tray_intent_is_emitted_with_its_accepted_wire_id() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let mut value: Value = serde_json::from_slice(extension_settings_sync_with_tray_action())?;
    value["settings"]["trayActions"]["actions"][0]["id"] =
        Value::String("capture-screen".to_owned());
    let payload = serde_json::to_vec(&value)?;
    let message = parse_wire_message(&payload)?;

    let accepted = responses_for_message(&mut session, &message, &value);
    assert_eq!(accepted[0]["type"], "app.settings.accepted");
    let values = responses_for_synced_tray_action(
        &session,
        "capture-screen",
        true,
        "invocation-legacy",
        5_001,
    )?;

    assert_eq!(values[0]["type"], "app.tray.actionRequested");
    assert_eq!(values[0]["actionId"], "capture-screen");
    parse_wire_message(&serde_json::to_vec(&values[0])?)?;
    Ok(())
}

#[test]
fn disabled_registry_entry_rejects_a_stale_tray_event() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let mut value: Value = serde_json::from_slice(extension_settings_sync_with_tray_action())?;
    value["settings"]["trayActions"]["actions"][0]["enabled"] = Value::Bool(false);
    let payload = serde_json::to_vec(&value)?;
    let message = parse_wire_message(&payload)?;
    responses_for_message(&mut session, &message, &value);

    assert!(matches!(
        responses_for_synced_tray_action(&session, "screenshot-screen", true, "stale", 5_002),
        Err(crate::tray::TrayActionError::Disabled)
    ));
    Ok(())
}
