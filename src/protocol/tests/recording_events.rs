use crate::protocol::recording_events::{EventRecord, decode_event_block};

fn fixture() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(include_str!(
        "../../../contracts/native/fixtures/recording-events-v1.json"
    ))?)
}

#[test]
fn original_event_bytes_admit_all_families_and_nullable_privacy_variants()
-> Result<(), Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(include_bytes!(
                "../../../contracts/native/fixtures/recording-events-v1.json"
            ))
        ),
        crate::protocol::recording_events::RECORDING_EVENTS_FIXTURE_SHA
    );
    let fixture = fixture()?;
    let mut count = 0;
    for source in fixture["blocks"].as_array().ok_or("blocks")? {
        let bytes = source["utf8"].as_str().ok_or("bytes")?.as_bytes();
        let block = decode_event_block(bytes).map_err(|error| format!("{error:?}"))?;
        assert_eq!(serde_json::to_value(&block)?, source["value"]);
        count += block.records.len();
    }
    assert_eq!(count, 18);
    let bytes = fixture["blocks"][2]["utf8"].as_str().ok_or("bytes")?;
    let block = decode_event_block(bytes.as_bytes()).map_err(|error| format!("{error:?}"))?;
    assert!(matches!(block.records[3], EventRecord::Signal { .. }));
    assert_eq!(block.records[3].position().2, 900_000);
    assert_eq!(block.records[4].position().2, 250_000);
    Ok(())
}

#[test]
fn click_completion_has_exact_required_modifier_fields_and_never_rewrites_click_count()
-> Result<(), Box<dyn std::error::Error>> {
    let mut block = fixture()?["blocks"][0]["value"].clone();
    let update = serde_json::json!({
        "kind": "action-update", "recordSeq": 7, "segmentId": 0,
        "activeTimeUs": 120_000, "actionId": "click-1", "revision": 1,
        "data": {
            "gestureType": "click", "completion": "released", "upTimeUs": 110_000,
            "pressDurationUs": null, "lastObservedTimeUs": 110_000,
            "semanticEndTimeUs": null, "dragStartTimeUs": null,
            "pointerCancelled": null, "dropObserved": null,
            "upPoint": {"x": 0.5, "y": 0.25},
            "ctrlKey": true, "altKey": false, "shiftKey": null, "metaKey": null
        }
    });
    let records = block["records"].as_array_mut().ok_or("records")?;
    records.insert(7, update.clone());
    for (index, record) in records.iter_mut().enumerate() {
        record["recordSeq"] = serde_json::json!(index);
    }
    let decoded =
        decode_event_block(&serde_json::to_vec(&block)?).map_err(|error| format!("{error:?}"))?;
    assert_eq!(serde_json::to_value(decoded)?["records"][7], update);
    for field in ["ctrlKey", "altKey", "shiftKey", "metaKey"] {
        let mut missing = block.clone();
        missing["records"][7]["data"]
            .as_object_mut()
            .ok_or("data")?
            .remove(field);
        assert!(
            decode_event_block(&serde_json::to_vec(&missing)?).is_err(),
            "{field}"
        );
    }
    let mut forbidden = block;
    forbidden["records"][7]["data"]["clickCount"] = serde_json::json!(2);
    assert!(decode_event_block(&serde_json::to_vec(&forbidden)?).is_err());
    Ok(())
}

#[test]
fn original_integer_tokens_are_checked_before_float_rounding()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let bytes = fixture["blocks"][2]["utf8"].as_str().ok_or("bytes")?;
    for token in [
        "9007199254740990.9",
        "7e4",
        "70000.0",
        "-0",
        "9007199254740992",
    ] {
        let mutated = bytes.replacen(
            "\"eventCount\":70000",
            &format!("\"eventCount\":{token}"),
            1,
        );
        assert_ne!(mutated, bytes);
        assert!(decode_event_block(mutated.as_bytes()).is_err(), "{token}");
    }
    let exponent_time = bytes.replacen("\"activeTimeUs\":250000", "\"activeTimeUs\":25e4", 1);
    assert!(decode_event_block(exponent_time.as_bytes()).is_err());
    Ok(())
}

#[test]
fn duplicate_unknown_missing_and_private_fields_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let bytes = fixture["blocks"][2]["utf8"].as_str().ok_or("bytes")?;
    for (from, to) in [
        (
            "\"eventCount\":70000",
            "\"eventCount\":70000,\"eventCount\":1",
        ),
        (
            "\"privacy\":\"unknown\"",
            "\"privacy\":\"unknown\",\"targetId\":\"private\"",
        ),
        ("\"upTimeUs\":null,", ""),
        ("\"resolutionBoundUs\":null,", ""),
        ("\"eventCount\":null", "\"eventCount\":1"),
        ("\"endTimeUs\":1000000", "\"endTimeUs\":999999"),
    ] {
        let mutated = bytes.replacen(from, to, 1);
        assert_ne!(mutated, bytes, "{from}");
        assert!(decode_event_block(mutated.as_bytes()).is_err(), "{from}");
    }
    assert!(decode_event_block(format!("{bytes} {{}}").as_bytes()).is_err());
    assert!(decode_event_block(&vec![b' '; 65_537]).is_err());
    Ok(())
}

#[test]
fn event_packing_has_no_lifetime_limit_and_delivery_cannot_add_seals()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::protocol::recording_events::{EventBlock, EventPacker};
    let fixture = fixture()?;
    let block: EventBlock = serde_json::from_value(fixture["blocks"][0]["value"].clone())?;
    let prototype = block.records[1].clone();
    let mut packer = EventPacker::new("history-packing".to_owned(), "a".repeat(32));
    let mut received = 0;
    let mut blocks = 0;
    for index in 0..5000_u64 {
        let mut record = prototype.clone();
        if let EventRecord::Action {
            record_seq,
            active_time_us,
            action,
            ..
        } = &mut record
        {
            *record_seq = index;
            *active_time_us = index;
            action.time_us = index;
            action.id = format!("action-{index}");
        }
        let sealed = packer.push(record).map_err(|error| format!("{error:?}"))?;
        assert!(packer.pending_records() <= 512);
        for bytes in sealed {
            assert!(bytes.len() <= 65536);
            let block = decode_event_block(&bytes).map_err(|error| format!("{error:?}"))?;
            assert_eq!(block.sequence, blocks);
            for record in block.records {
                assert_eq!(record.position().0, received);
                received += 1;
            }
            blocks += 1;
        }
    }
    for bytes in packer.finish().map_err(|error| format!("{error:?}"))? {
        let block = decode_event_block(&bytes).map_err(|error| format!("{error:?}"))?;
        assert_eq!(block.sequence, blocks);
        for record in block.records {
            assert_eq!(record.position().0, received);
            received += 1;
        }
    }
    assert_eq!(received, 5000);
    assert_eq!(
        packer.finish().map_err(|error| format!("{error:?}"))?,
        [] as [std::vec::Vec<u8>; 0]
    );
    assert!(packer.push(prototype).is_err());
    Ok(())
}

#[test]
fn event_receipt_is_independent_closed_and_matches_every_committed_count()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::protocol::recording_events::{EventDescriptor, EventReceipt};
    let fixture = fixture()?;
    let descriptor: EventDescriptor = serde_json::from_value(fixture["descriptor"].clone())?;
    descriptor
        .validate()
        .map_err(|error| format!("{error:?}"))?;
    let receipt: EventReceipt = serde_json::from_value(fixture["receipt"].clone())?;
    assert!(receipt.matches(&descriptor));
    for (key, value) in [
        ("version", serde_json::json!(2)),
        ("durable", serde_json::json!(false)),
        ("state", serde_json::json!("complete")),
        ("totalBlocks", serde_json::json!(4)),
        ("totalRecords", serde_json::json!(19)),
        ("totalBytes", serde_json::json!(5529)),
        ("streamSha256", serde_json::json!("a".repeat(64))),
    ] {
        let mut wrong = fixture["receipt"].clone();
        wrong[key] = value;
        let wrong: EventReceipt = serde_json::from_value(wrong)?;
        assert!(!wrong.matches(&descriptor), "{key}");
    }
    let mut wrong = fixture["receipt"].clone();
    wrong["totalSamples"] = serde_json::json!(0);
    assert!(serde_json::from_value::<EventReceipt>(wrong).is_err());
    let mut pending = descriptor.clone();
    pending.finalized = false;
    assert!(!receipt.matches(&pending));
    Ok(())
}

#[test]
fn browser_and_native_command_labels_share_a_closed_non_text_grammar() {
    use crate::protocol::recording_events::valid_event_key_label;
    for label in [
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "Insert",
        "Shift+Tab",
        "Ctrl+Home",
        "Alt+Shift",
        "Ctrl",
        "Ctrl+Alt+Shift+Meta+F24",
        "Ctrl+S",
        "ArrowUp",
        "Unknown",
    ] {
        assert!(valid_event_key_label(label), "{label}");
    }
    for label in [
        "Shift+A",
        "A",
        "typed text",
        "Ctrl+password",
        "Ctrl+Ctrl+A",
        "F01",
        "F25",
        "Up",
        "Alt+Shift+Alt",
        "Ctrl+;",
        "Ctrl++",
        "Ctrl+é",
        "Ctrl+",
        "",
    ] {
        assert!(!valid_event_key_label(label), "{label}");
    }
}

#[test]
fn utf8_bom_is_rejected_without_normalizing_original_event_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let bytes = fixture["blocks"][0]["utf8"]
        .as_str()
        .ok_or("bytes")?
        .as_bytes();
    let mut prefixed = vec![0xef, 0xbb, 0xbf];
    prefixed.extend_from_slice(bytes);
    assert!(decode_event_block(&prefixed).is_err());
    Ok(())
}

#[test]
fn completed_anonymous_keyboard_holds_match_shared_original_byte_vectors()
-> Result<(), Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    let source =
        include_bytes!("../../../contracts/native/fixtures/recording-events-keyboard-hold-v1.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(source)),
        "b76ae166fb09777a4b5a4efd504c8bfe53fad15540ca05305bf0589645f1b634"
    );
    let vectors: serde_json::Value = serde_json::from_slice(source)?;
    for case in vectors["cases"].as_array().ok_or("cases")? {
        let name = case["name"].as_str().ok_or("name")?;
        let bytes = case["utf8"].as_str().ok_or("bytes")?.as_bytes();
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), case["sha256"]);
        let decoded = decode_event_block(bytes);
        assert_eq!(
            decoded.is_ok(),
            case["accepted"].as_bool().ok_or("accepted")?,
            "{name}"
        );
        if let Ok(block) = decoded {
            let value = serde_json::to_value(block)?;
            let original: serde_json::Value = serde_json::from_slice(bytes)?;
            assert_eq!(value, original, "{name}");
        }
    }
    Ok(())
}

#[test]
fn non_wire_event_contract_fixtures_reach_their_real_owner_guards()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::protocol::{parse_wire_message, recording_events::EventDescriptor};
    for valid in [true, false] {
        let bytes = if valid {
            include_str!("../../../contracts/native/fixtures/positive/contract-fixtures.json")
        } else {
            include_str!("../../../contracts/native/fixtures/negative/contract-fixtures.json")
        };
        let values: serde_json::Value = serde_json::from_str(bytes)?;
        let accepted =
            decode_event_block(&serde_json::to_vec(&values["RecordingEventsBlock"])?).is_ok();
        assert_eq!(accepted, valid, "block");
        let accepted =
            serde_json::from_value::<EventDescriptor>(values["RecordingEventsDescriptor"].clone())
                .is_ok_and(|descriptor| descriptor.validate().is_ok());
        assert_eq!(accepted, valid, "descriptor");
        let positive: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/native/fixtures/positive/contract-fixtures.json"
        ))?;
        let mut ack = positive["ExtensionRecordingEventsAckMessage"].clone();
        ack["receipt"] = values["RecordingEventsReceipt"].clone();
        assert_eq!(
            parse_wire_message(&serde_json::to_vec(&ack)?).is_ok(),
            valid,
            "receipt"
        );
        let mut hello = positive["AppHelloMessage"].clone();
        hello["capabilities"]["eventAttachments"] =
            values["NativeEventAttachmentCapabilities"].clone();
        assert_eq!(
            parse_wire_message(&serde_json::to_vec(&hello)?).is_ok(),
            valid,
            "capability"
        );
    }
    Ok(())
}

#[test]
fn event_wire_pages_have_independent_ranges_and_closed_chunk_envelopes()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::protocol::parse_wire_message;
    let values: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/native/fixtures/positive/contract-fixtures.json"
    ))?;
    let mut page = values["AppRecordingEventsManifestMessage"].clone();
    page["entries"][1]["startTimeUs"] = serde_json::json!(0);
    assert!(parse_wire_message(&serde_json::to_vec(&page)?).is_ok());
    page.as_object_mut().ok_or("page")?.remove("nextSequence");
    assert!(parse_wire_message(&serde_json::to_vec(&page)?).is_err());
    let chunk = &values["AppRecordingEventsChunkMessage"];
    for (key, bad) in [
        ("chunkIndex", serde_json::json!(1)),
        ("chunkCount", serde_json::json!(2)),
        ("chunkByteOffset", serde_json::json!(1)),
        ("chunkRawBytes", serde_json::json!(32769)),
        ("blockBytes", serde_json::json!(65537)),
        ("blockSha256", serde_json::json!("A".repeat(64))),
        ("sequence", serde_json::json!(9_007_199_254_740_992_u64)),
        ("privatePath", serde_json::json!("forbidden")),
    ] {
        let mut bad_chunk = chunk.clone();
        bad_chunk[key] = bad;
        assert!(
            parse_wire_message(&serde_json::to_vec(&bad_chunk)?).is_err(),
            "{key}"
        );
    }
    Ok(())
}
