use super::*;
use serde_json::json;

#[test]
fn capture_request_rejects_foreign_fields_and_incompatible_options() {
    for mut value in [
        json!({"command":"start","options":{"kind":"screenshot","target":"display","quality":"high"}}),
        json!({"command":"start","options":{"kind":"video","target":"region"}}),
        json!({"command":"start","options":{"kind":"video","target":"region","quality":"medium"}}),
        json!({"command":"start","options":{"kind":"video","target":"region","quality":"high","path":"private"}}),
        json!({"command":"pause","options":{}}),
        json!({"command":"stop","operationId":"foreign"}),
        json!({"command":"start","options":{"kind":"screenshot","target":"all-screens"}}),
        json!({"command":"execute","args":[]}),
        json!({"command":"start","options":null}),
        json!({"command":"start","options":{"kind":"screenshot","target":null}}),
    ] {
        value["expectedEpoch"] = json!(0);
        assert!(
            serde_json::from_value::<CaptureRequest>(value.clone()).is_err(),
            "{value}"
        );
    }
}

#[test]
fn capture_request_accepts_only_the_internal_typed_actions() -> Result<(), serde_json::Error> {
    for command in ["pause", "resume", "stop", "discard"] {
        let value = json!({"command":command,"expectedEpoch":0});
        let request: CaptureRequest = serde_json::from_value(value.clone())?;
        assert_eq!(serde_json::to_value(request)?, value);
    }
    for target in ["region", "display", "window"] {
        for options in [
            json!({"kind":"screenshot","target":target}),
            json!({"kind":"video","target":target,"quality":"balanced"}),
        ] {
            let value = json!({"command":"start","options":options,"expectedEpoch":0});
            let request: CaptureRequest = serde_json::from_value(value.clone())?;
            assert_eq!(serde_json::to_value(request)?, value);
        }
    }
    Ok(())
}

#[test]
fn preference_updates_reject_nulls_unknown_locales_and_extra_fields()
-> Result<(), serde_json::Error> {
    for value in [
        json!({"theme":null}),
        json!({"locale":null}),
        json!({"theme":"dark","locale":"xx"}),
        json!({"theme":"dark","destination":"foreign"}),
    ] {
        assert!(
            serde_json::from_value::<PreferencesPatch>(value.clone()).is_err(),
            "{value}"
        );
    }
    let patch: PreferencesPatch = serde_json::from_value(json!({"locale":"ru"}))?;
    assert_eq!(patch.theme, None);
    assert_eq!(patch.locale, Some(UiLocale::Russian));
    Ok(())
}

#[test]
fn capture_commands_require_a_nonnegative_integer_operation_epoch() {
    for epoch in [json!(null), json!(-1), json!(1.5), json!("1")] {
        assert!(
            serde_json::from_value::<CaptureRequest>(
                json!({"command":"discard","expectedEpoch":epoch})
            )
            .is_err()
        );
    }
    assert!(serde_json::from_value::<CaptureRequest>(json!({"command":"discard"})).is_err());
}
