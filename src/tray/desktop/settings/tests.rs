use super::SettingsRequest;
use serde_json::json;

#[test]
fn settings_requests_reject_foreign_actions_fields_and_missing_revisions() {
    for value in [
        json!({"command":"open", "destinationPath":"foreign"}),
        json!({"command":"save", "draft":{}}),
        json!({"command":"apply", "expectedRevision":null, "draft":{}}),
        json!({"command":"apply", "expectedRevision":-1, "draft":{}}),
        json!({"command":"apply", "draft":{}}),
        json!({"command":"choose-folder", "path":"foreign"}),
        json!({"command":"use-extension", "expectedRevision":null}),
        json!({"command":"use-extension", "expectedRevision":1, "foreign":true}),
        json!({"command":"save-delivery", "askAfterCapture":true}),
        json!({"command":"save-delivery", "targetId":null, "askAfterCapture":null}),
    ] {
        assert!(serde_json::from_value::<SettingsRequest>(value).is_err());
    }
    for command in [
        "open",
        "choose-folder",
        "refresh-devices",
        "refresh-browsers",
    ] {
        assert!(serde_json::from_value::<SettingsRequest>(json!({"command":command})).is_ok());
    }
}
