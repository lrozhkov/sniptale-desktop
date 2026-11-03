use crate::{
    protocol::{
        recording_events::{
            EventAlgorithm, EventDescriptor, EventEncoding, EventFormat, EventStreamValidator,
            decode_event_block,
        },
        trajectory::CursorTrajectoryState,
    },
    telemetry::{
        CursorTrajectorySinkError, NativeRecordingEvents, RecordingEventsSink,
        RecordingEventsSinkFactory, RecordingFrameGeometry, RecordingTelemetryPolicy,
        RecordingTrajectoryContext,
        trajectory_clock::WindowsTrajectoryClock,
        windows::{WindowsKeyEvent, WindowsKeyModifiers, WindowsKeyObservation, WindowsKeyPhase},
    },
};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

type ProofError = Box<dyn std::error::Error>;

#[derive(Debug)]
struct MemoryProof {
    descriptor: EventDescriptor,
    validator: EventStreamValidator,
    stream: Sha256,
    blocks: Vec<Vec<u8>>,
    erase_fails: bool,
}

#[derive(Clone, Debug)]
struct MemoryFactory(Arc<Mutex<MemoryProof>>);
#[derive(Debug)]
struct MemorySink(Arc<Mutex<MemoryProof>>);

impl RecordingEventsSinkFactory for MemoryFactory {
    fn create(
        &mut self,
        _: &str,
        _: &str,
        _: &RecordingTrajectoryContext,
    ) -> Result<Box<dyn RecordingEventsSink>, CursorTrajectorySinkError> {
        Ok(Box::new(MemorySink(self.0.clone())))
    }
}

impl RecordingEventsSink for MemorySink {
    fn try_append(&mut self, bytes: Vec<u8>) -> Result<(), CursorTrajectorySinkError> {
        let block = decode_event_block(&bytes).map_err(|_| CursorTrajectorySinkError::Storage)?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| CursorTrajectorySinkError::Storage)?;
        state
            .validator
            .accept(&block)
            .map_err(|_| CursorTrajectorySinkError::Storage)?;
        let length = u32::try_from(bytes.len()).map_err(|_| CursorTrajectorySinkError::Storage)?;
        state.stream.update(length.to_le_bytes());
        state.stream.update(&bytes);
        state.descriptor.total_blocks += 1;
        state.descriptor.total_records +=
            u64::try_from(block.records.len()).map_err(|_| CursorTrajectorySinkError::Storage)?;
        state.descriptor.total_bytes += u64::from(length);
        state.descriptor.stream_sha256 = format!("{:x}", state.stream.clone().finalize());
        state.blocks.push(bytes);
        Ok(())
    }
    fn finish(&mut self, incomplete: bool) -> EventDescriptor {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.descriptor.finalized = true;
        if incomplete || state.validator.incomplete() || !state.validator.closed_prefix() {
            state.descriptor.state = CursorTrajectoryState::Incomplete;
        }
        state.descriptor.clone()
    }
    fn erase(&mut self) -> Result<(), CursorTrajectorySinkError> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| CursorTrajectorySinkError::Storage)?;
        if state.erase_fails {
            return Err(CursorTrajectorySinkError::Storage);
        }
        state.blocks.clear();
        Ok(())
    }
}

fn recorder() -> Result<(NativeRecordingEvents, MemoryFactory), ProofError> {
    recorder_with_mapping(crate::telemetry::RecordingPointMapping::FixedDesktopFrame)
}

fn recorder_with_mapping(
    point_mapping: crate::telemetry::RecordingPointMapping,
) -> Result<(NativeRecordingEvents, MemoryFactory), ProofError> {
    let mut factory = MemoryFactory(Arc::new(Mutex::new(MemoryProof {
        descriptor: EventDescriptor {
            format: EventFormat::V1,
            format_version: 1,
            algorithm: EventAlgorithm::LosslessV1,
            encoding: EventEncoding::JsonUtf8V1,
            event_stream_id: "a".repeat(32),
            state: CursorTrajectoryState::Complete,
            finalized: false,
            total_blocks: 0,
            total_records: 0,
            total_bytes: 0,
            stream_sha256: format!("{:x}", Sha256::digest([])),
        },
        validator: EventStreamValidator::default(),
        stream: Sha256::new(),
        blocks: Vec::new(),
        erase_fails: false,
    })));
    let context = RecordingTrajectoryContext {
        point_mapping,
        frame: RecordingFrameGeometry {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        },
        delivery_route: crate::protocol::DeliveryRoute::DesktopDelivery {
            asset_job_id: "events-proof".to_owned(),
            target_id: Some("a".repeat(64)),
            ask_after_capture: false,
        },
    };
    let clock =
        WindowsTrajectoryClock::calibrate(1000, 0, 0).map_err(|error| format!("{error:?}"))?;
    let recorder = NativeRecordingEvents::new(
        "native-events-proof",
        "a".repeat(32),
        &context,
        RecordingTelemetryPolicy::all_enabled_for_tests(),
        0,
        Some(clock),
        Some(&mut factory),
    );
    Ok((recorder, factory))
}

fn key(
    sequence: u64,
    time_ms: u64,
    phase: WindowsKeyPhase,
    virtual_key: u32,
) -> WindowsKeyObservation {
    WindowsKeyObservation {
        sequence,
        source_time_ms: 1000 + time_ms,
        phase,
        event: WindowsKeyEvent {
            virtual_key,
            modifiers: WindowsKeyModifiers::default(),
        },
    }
}

#[test]
fn joining_sources_keeps_an_accepted_pointer_fact_before_a_later_keyboard_gap()
-> Result<(), ProofError> {
    use crate::telemetry::{
        WindowsInputObservation, WindowsMouseButton as Button, WindowsMouseEventKind as Kind,
        merge_input_observations,
    };
    let (mut recorder, factory) = recorder()?;
    let keyboard = vec![
        key(0, 1, WindowsKeyPhase::Down, 0x25),
        key(2, 3, WindowsKeyPhase::Repeat, 0x25),
    ];
    let pointer = vec![
        mouse(0, 2, Kind::Down(Button::Left), 64, 36),
        mouse(1, 4, Kind::Up(Button::Left), 64, 36),
    ];
    for event in merge_input_observations(keyboard, pointer) {
        match event {
            WindowsInputObservation::Keyboard(event) => recorder.keyboard(event),
            WindowsInputObservation::Mouse(event) => recorder.mouse(event),
        }
        .map_err(|error| format!("{error:?}"))?;
    }
    assert_eq!(
        recorder.finish(10_000_000).state,
        CursorTrajectoryState::Incomplete
    );
    let records = records(&factory)?;
    let click = records
        .iter()
        .position(|record| record["action"]["kind"] == "CLICK")
        .ok_or("accepted click")?;
    let gap = records
        .iter()
        .position(|record| record["kind"] == "gap")
        .ok_or("keyboard gap")?;
    assert!(click < gap);
    assert_eq!(records[click]["segmentId"], 0);
    assert_eq!(records[click]["activeTimeUs"], 2000);
    assert_eq!(records[gap]["firstLostSourceSeq"], 1);
    assert_eq!(records[gap]["lostCount"], 1);
    Ok(())
}

#[test]
fn source_join_preserves_equal_time_facts_and_each_producer_order() {
    use crate::telemetry::{
        WindowsInputObservation, WindowsMouseEventKind as Kind, merge_input_observations,
    };
    let keyboard = vec![
        key(4, 2, WindowsKeyPhase::Down, 0x25),
        key(5, 1, WindowsKeyPhase::Up, 0x25),
    ];
    let pointer = vec![mouse(8, 2, Kind::Move, 1, 1), mouse(9, 3, Kind::Move, 2, 2)];
    let joined: Vec<_> = merge_input_observations(keyboard, pointer)
        .map(|event| match event {
            WindowsInputObservation::Keyboard(event) => {
                ("keyboard", event.sequence, event.source_time_ms)
            }
            WindowsInputObservation::Mouse(event) => {
                ("mouse", event.sequence, event.source_time_ms)
            }
        })
        .collect();
    assert_eq!(
        joined,
        [
            ("keyboard", 4, 1002),
            ("keyboard", 5, 1001),
            ("mouse", 8, 1002),
            ("mouse", 9, 1003)
        ]
    );
}

fn records(factory: &MemoryFactory) -> Result<Vec<serde_json::Value>, ProofError> {
    let state = factory.0.lock().map_err(|_| "proof state")?;
    let mut records = Vec::new();
    for bytes in &state.blocks {
        assert!(bytes.len() <= 65536);
        let block = decode_event_block(bytes).map_err(|error| format!("{error:?}"))?;
        for record in block.records {
            records.push(serde_json::to_value(record)?);
        }
    }
    Ok(records)
}

#[test]
fn native_command_hold_preserves_seventy_thousand_repeats_and_stop_does_not_forge_keyup()
-> Result<(), ProofError> {
    let (mut recorder, factory) = recorder()?;
    recorder
        .keyboard(key(0, 0, WindowsKeyPhase::Down, 0x74))
        .map_err(|error| format!("{error:?}"))?;
    for sequence in 1..=70_000 {
        recorder
            .keyboard(key(sequence, sequence, WindowsKeyPhase::Repeat, 0x74))
            .map_err(|error| format!("{error:?}"))?;
    }
    let descriptor = recorder.finish(600_000_000_000);
    assert_eq!(descriptor.state, CursorTrajectoryState::Complete);
    let records = records(&factory)?;
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == "action")
            .count(),
        1
    );
    let cadence: Vec<_> = records
        .iter()
        .filter(|record| record["kind"] == "keyboard-repeat-block" && record["phase"] == "repeat")
        .collect();
    assert_eq!(cadence.len(), 137);
    assert_eq!(
        cadence
            .iter()
            .filter_map(|record| record["cadence"]["count"].as_u64())
            .sum::<u64>(),
        70_000
    );
    assert_eq!(
        cadence.last().ok_or("last cadence")?["cadence"]["count"],
        368
    );
    let update = records
        .iter()
        .find(|record| record["kind"] == "action-update")
        .ok_or("completion")?;
    assert_eq!(update["data"]["repeatCount"], 70_000);
    assert_eq!(update["data"]["lastObservedTimeUs"], 70_000_000);
    assert!(update["data"]["upTimeUs"].is_null());
    assert!(update["data"]["pressDurationUs"].is_null());
    Ok(())
}

#[test]
fn unknown_native_typing_keeps_exact_counts_without_new_per_key_cadence() -> Result<(), ProofError>
{
    let (mut recorder, factory) = recorder()?;
    for sequence in 0..70_000 {
        recorder
            .keyboard(key(
                sequence,
                sequence,
                WindowsKeyPhase::UnclassifiedDown,
                0x41,
            ))
            .map_err(|error| format!("{error:?}"))?;
    }
    assert_eq!(
        recorder.finish(600_000_000_000).state,
        CursorTrajectoryState::Complete
    );
    let records = records(&factory)?;
    let typing: Vec<_> = records
        .iter()
        .filter(|record| record["kind"] == "signal")
        .collect();
    assert_eq!(typing.len(), 2);
    assert_eq!(typing[0]["signal"]["data"]["eventCount"], 65535);
    assert_eq!(typing[1]["signal"]["data"]["eventCount"], 4465);
    assert_eq!(typing[1]["signal"]["endTimeUs"], 69_999_000);
    assert!(
        typing
            .iter()
            .all(|record| record["signal"]["data"]["privacy"] == "unknown")
    );
    assert!(!records.iter().any(|record| matches!(
        record["kind"].as_str(),
        Some("action" | "keyboard-repeat-block")
    )));
    assert_eq!(
        typing[1]["signal"]["data"]["continuationOf"],
        typing[0]["signal"]["id"]
    );
    Ok(())
}

#[test]
fn pause_then_stop_closes_the_existing_prefix_without_a_resume_or_new_observation()
-> Result<(), ProofError> {
    let (mut recorder, factory) = recorder()?;
    recorder
        .keyboard(key(0, 1, WindowsKeyPhase::Down, 0x74))
        .map_err(|error| format!("{error:?}"))?;
    recorder
        .pause(2_000_000)
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        recorder.finish(600_000_000_000).state,
        CursorTrajectoryState::Complete
    );
    let records = records(&factory)?;
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == "segment")
            .count(),
        1
    );
    assert_eq!(records.last().ok_or("pause")?["reason"], "pause");
    let update = records
        .iter()
        .find(|record| record["kind"] == "action-update")
        .ok_or("completion")?;
    assert_eq!(update["data"]["lastObservedTimeUs"], 1000);
    Ok(())
}

fn mouse(
    sequence: u64,
    time_ms: u64,
    kind: crate::telemetry::WindowsMouseEventKind,
    x: i32,
    y: i32,
) -> crate::telemetry::WindowsMouseEvent {
    crate::telemetry::WindowsMouseEvent {
        sequence,
        source_time_ms: 1000 + time_ms,
        visible: Some(true),
        x,
        y,
        kind,
    }
}

#[test]
fn pointer_completion_uses_real_motion_and_proven_up_coordinates() -> Result<(), ProofError> {
    use crate::telemetry::{WindowsMouseButton as Button, WindowsMouseEventKind as Kind};
    for released in [false, true] {
        let (mut recorder, factory) = recorder()?;
        recorder
            .mouse(mouse(0, 1000, Kind::Down(Button::Left), 64, 36))
            .map_err(|error| format!("{error:?}"))?;
        recorder
            .mouse(mouse(1, 2000, Kind::Move, 128, 72))
            .map_err(|error| format!("{error:?}"))?;
        if released {
            recorder
                .mouse(mouse(2, 3000, Kind::Up(Button::Left), 256, 144))
                .map_err(|error| format!("{error:?}"))?;
        }
        assert_eq!(
            recorder.finish(30_000_000_000).state,
            CursorTrajectoryState::Complete
        );
        let records = records(&factory)?;
        let update = records
            .iter()
            .find(|record| record["kind"] == "action-update")
            .ok_or("completion")?;
        if released {
            assert_eq!(update["data"]["lastObservedTimeUs"], 3_000_000);
            assert_eq!(update["data"]["pressDurationUs"], 2_000_000);
            assert_eq!(
                update["data"]["upPoint"],
                serde_json::json!({"x":0.2,"y":0.2})
            );
        } else {
            assert_eq!(update["data"]["lastObservedTimeUs"], 2_000_000);
            assert!(update["data"]["upTimeUs"].is_null());
            assert!(update["data"]["pressDurationUs"].is_null());
            assert!(update["data"]["upPoint"].is_null());
        }
        assert!(update["data"]["gestureType"].is_null());
        assert!(update["data"]["dropObserved"].is_null());
        let data = update["data"].as_object().ok_or("completion fields")?;
        assert!(!data.contains_key("clickCount"));
        for field in ["ctrlKey", "altKey", "shiftKey", "metaKey"] {
            assert_eq!(data.get(field), Some(&serde_json::Value::Null));
        }
    }
    Ok(())
}

#[test]
fn five_thousand_nearby_clicks_keep_independent_actions_and_a_sticky_loss_marker()
-> Result<(), ProofError> {
    use crate::telemetry::{WindowsMouseButton as Button, WindowsMouseEventKind as Kind};
    let (mut recorder, factory) = recorder()?;
    recorder
        .warning("keyboard-hook", "events-dropped", 1_000_000)
        .map_err(|error| format!("{error:?}"))?;
    for index in 0..5000_u64 {
        recorder
            .mouse(mouse(
                index * 2,
                index * 2 + 2,
                Kind::Down(Button::Left),
                64,
                36,
            ))
            .map_err(|error| format!("{error:?}"))?;
        recorder
            .mouse(mouse(
                index * 2 + 1,
                index * 2 + 3,
                Kind::Up(Button::Left),
                64,
                36,
            ))
            .map_err(|error| format!("{error:?}"))?;
    }
    assert_eq!(
        recorder.finish(20_000_000_000).state,
        CursorTrajectoryState::Incomplete
    );
    let records = records(&factory)?;
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == "action" && record["action"]["kind"] == "CLICK")
            .count(),
        5000
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == "action-update")
            .count(),
        5000
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == "gap")
            .count(),
        1
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record["signal"]["data"]["code"] == "events-dropped")
            .count(),
        1
    );
    Ok(())
}

#[test]
fn accepted_warning_instances_are_not_thinned_at_the_old_signal_limit() -> Result<(), ProofError> {
    let (mut recorder, factory) = recorder()?;
    for index in 0..1025_u64 {
        recorder
            .warning("cursor-trajectory", "format-unavailable", index * 1_000_000)
            .map_err(|error| format!("{error:?}"))?;
    }
    assert_eq!(
        recorder.finish(2_000_000_000).state,
        CursorTrajectoryState::Complete
    );
    let records = records(&factory)?;
    let signals: Vec<_> = records
        .iter()
        .filter(|record| record["kind"] == "signal")
        .collect();
    assert_eq!(signals.len(), 1025);
    for (index, signal) in signals.iter().enumerate() {
        assert_eq!(signal["signal"]["startTimeUs"], index * 1000);
    }
    Ok(())
}

#[test]
fn an_observed_command_release_without_a_known_down_never_fabricates_an_onset()
-> Result<(), ProofError> {
    let (mut recorder, factory) = recorder()?;
    recorder
        .keyboard(key(0, 1000, WindowsKeyPhase::Up, 0x74))
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        recorder.finish(2_000_000_000).state,
        CursorTrajectoryState::Incomplete
    );
    let records = records(&factory)?;
    let release = records
        .iter()
        .find(|record| record["kind"] == "action")
        .ok_or("release")?;
    assert_eq!(release["action"]["data"]["upTimeUs"], 1_000_000);
    assert!(release["action"]["data"]["keyboardOnsetTimeUs"].is_null());
    assert!(release["action"]["data"]["keyboardAttackId"].is_null());
    assert!(release["action"]["data"]["downTimeUs"].is_null());
    assert!(release["action"]["data"]["pressDurationUs"].is_null());
    assert!(
        !records
            .iter()
            .any(|record| record["kind"] == "keyboard-repeat-block")
    );
    assert!(
        records
            .iter()
            .any(|record| record["kind"] == "gap" && record["reason"] == "source-unavailable")
    );
    Ok(())
}

#[test]
fn unknown_command_onsets_report_one_context_gap_without_fragmenting_each_key()
-> Result<(), ProofError> {
    let (mut recorder, factory) = recorder()?;
    for (index, virtual_key) in (0x70..=0x87).enumerate() {
        let sequence = u64::try_from(index)? * 2;
        recorder
            .keyboard(key(
                sequence,
                1000 + sequence,
                WindowsKeyPhase::UnclassifiedDown,
                virtual_key,
            ))
            .map_err(|error| format!("{error:?}"))?;
        recorder
            .keyboard(key(
                sequence + 1,
                1001 + sequence,
                WindowsKeyPhase::Up,
                virtual_key,
            ))
            .map_err(|error| format!("{error:?}"))?;
    }
    assert_eq!(
        recorder.finish(2_000_000_000).state,
        CursorTrajectoryState::Incomplete
    );
    let records = records(&factory)?;
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == "gap" && record["reason"] == "source-unavailable")
            .count(),
        1
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == "action")
            .count(),
        24
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == "action-update")
            .count(),
        24
    );
    assert!(
        !records
            .iter()
            .any(|record| record["kind"] == "keyboard-repeat-block")
    );
    Ok(())
}

#[test]
fn an_unknown_keyboard_onset_preserves_other_proven_input_pairs_across_the_context_gap()
-> Result<(), ProofError> {
    use crate::telemetry::{WindowsMouseButton as Button, WindowsMouseEventKind as Kind};
    let (mut recorder, factory) = recorder()?;
    recorder
        .keyboard(key(0, 1000, WindowsKeyPhase::Down, 0x74))
        .map_err(|error| format!("{error:?}"))?;
    recorder
        .mouse(mouse(0, 1100, Kind::Down(Button::Left), 64, 36))
        .map_err(|error| format!("{error:?}"))?;
    recorder
        .keyboard(key(1, 1200, WindowsKeyPhase::Repeat, 0x75))
        .map_err(|error| format!("{error:?}"))?;
    recorder
        .mouse(mouse(1, 1300, Kind::Up(Button::Left), 128, 72))
        .map_err(|error| format!("{error:?}"))?;
    recorder
        .keyboard(key(2, 1400, WindowsKeyPhase::Up, 0x74))
        .map_err(|error| format!("{error:?}"))?;
    recorder
        .keyboard(key(3, 1500, WindowsKeyPhase::Up, 0x75))
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        recorder.finish(2_000_000_000).state,
        CursorTrajectoryState::Incomplete
    );
    let records = records(&factory)?;
    let click = records
        .iter()
        .find(|record| record["action"]["kind"] == "CLICK")
        .ok_or("click")?;
    let click_end = records
        .iter()
        .find(|record| {
            record["kind"] == "action-update" && record["actionId"] == click["action"]["id"]
        })
        .ok_or("click completion")?;
    assert_eq!(click_end["data"]["pressDurationUs"], 200_000);
    assert_eq!(
        click_end["data"]["upPoint"],
        serde_json::json!({"x":0.1,"y":0.1})
    );
    assert_ne!(click["segmentId"], click_end["segmentId"]);
    let known = records
        .iter()
        .find(|record| record["action"]["label"] == "F5")
        .ok_or("known command")?;
    let known_end = records
        .iter()
        .find(|record| {
            record["kind"] == "action-update" && record["actionId"] == known["action"]["id"]
        })
        .ok_or("known completion")?;
    assert_eq!(known_end["data"]["pressDurationUs"], 400_000);
    let unknown = records
        .iter()
        .find(|record| record["action"]["label"] == "F6")
        .ok_or("unknown command")?;
    assert!(unknown["action"]["data"]["downTimeUs"].is_null());
    assert!(unknown["action"]["data"]["keyboardAttackId"].is_null());
    let unknown_end = records
        .iter()
        .find(|record| {
            record["kind"] == "action-update" && record["actionId"] == unknown["action"]["id"]
        })
        .ok_or("unknown completion")?;
    assert!(unknown_end["data"]["pressDurationUs"].is_null());
    assert_eq!(unknown_end["data"]["repeatCount"], 1);
    Ok(())
}

#[test]
fn an_untracked_window_preserves_button_facts_without_inventing_frame_coordinates()
-> Result<(), ProofError> {
    use crate::telemetry::{WindowsMouseButton as Button, WindowsMouseEventKind as Kind};
    let mapping = crate::telemetry::RecordingPointMapping::for_capture_mode(
        &crate::protocol::VideoMode::ActiveWindow,
    );
    let (mut recorder, factory) = recorder_with_mapping(mapping)?;
    recorder
        .mouse(mouse(0, 1000, Kind::Down(Button::Left), 640, 360))
        .map_err(|error| format!("{error:?}"))?;
    recorder
        .mouse(mouse(1, 2000, Kind::Up(Button::Left), 100, 100))
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        recorder.finish(3_000_000_000).state,
        CursorTrajectoryState::Complete
    );
    let records = records(&factory)?;
    let click = records
        .iter()
        .find(|record| record["action"]["kind"] == "CLICK")
        .ok_or("click")?;
    assert!(click["action"]["point"].is_null());
    assert_eq!(click["action"]["timeUs"], 1_000_000);
    let released = records
        .iter()
        .find(|record| record["kind"] == "action-update")
        .ok_or("release")?;
    assert!(released["data"]["upPoint"].is_null());
    assert_eq!(released["data"]["upTimeUs"], 2_000_000);
    assert_eq!(released["data"]["pressDurationUs"], 1_000_000);
    assert_eq!(released["data"]["completion"], "released");
    Ok(())
}

#[test]
fn failed_cancel_and_discard_erasure_emit_a_category_without_sensitive_identifiers()
-> Result<(), ProofError> {
    use crate::telemetry::{RecordingTelemetrySource, RuntimeRecordingTelemetrySource};
    use std::io::{Read, Seek};
    let log = Arc::new(tempfile::tempfile()?);
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_writer(log.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, || -> Result<(), ProofError> {
        for discard in [false, true] {
            let (_, factory) = recorder()?;
            factory.0.lock().map_err(|_| "state")?.erase_fails = true;
            let context = RecordingTrajectoryContext {
                point_mapping: crate::telemetry::RecordingPointMapping::FixedDesktopFrame,
                frame: RecordingFrameGeometry {
                    x: 0,
                    y: 0,
                    width: 1280,
                    height: 720,
                },
                delivery_route: crate::protocol::DeliveryRoute::ExtensionTransfer,
            };
            let mut source =
                RuntimeRecordingTelemetrySource::with_trajectory_factory(|| 0, || 1000, None)
                    .with_events_factory(Box::new(factory));
            source.configure_trajectory(Some(context));
            source.configure_recording(RecordingTelemetryPolicy {
                collect_key_events: true,
                ..RecordingTelemetryPolicy::default()
            });
            let recording = "sensitive-recording-for-cleanup-proof";
            source.start_recording(recording, 0);
            if discard {
                source.discard_recording(recording);
            } else {
                source.cancel_recording(recording, 1_000_000);
            }
            assert!(source.snapshot_for_recording(recording, 0).is_none());
        }
        Ok(())
    })?;
    let mut input = log.as_ref();
    input.rewind()?;
    let mut text = String::new();
    input.read_to_string(&mut text)?;
    assert_eq!(
        text.matches("recording history cleanup remains pending")
            .count(),
        2
    );
    assert!(text.contains("recording-events"));
    assert!(!text.contains("sensitive-recording-for-cleanup-proof"));
    Ok(())
}
