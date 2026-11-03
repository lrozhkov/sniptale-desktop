use crate::telemetry::trajectory::{
    MAX_WINDOW_OBSERVATIONS, SedObservation, TrajectoryWindowError, retained_indices,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn source_point(value: &Value) -> Result<SedObservation, Box<dyn std::error::Error>> {
    Ok(SedObservation {
        observation_seq: value["observationSeq"].as_u64().ok_or("sequence")?,
        segment_id: value["segmentId"].as_u64().ok_or("segment")?,
        active_time_us: value["activeTimeUs"].as_u64().ok_or("time")?,
        position_q8: value["xQ8"].as_u64().zip(value["yQ8"].as_u64()),
        visible: value["visible"].as_bool().ok_or("visible")?,
        button_event: value["event"].as_str().ok_or("event")? != "move",
        semantic_anchor: !value["anchors"].as_array().ok_or("anchors")?.is_empty(),
    })
}

#[test]
fn shared_sed_vectors_retain_exact_source_sequences_and_preserve_input()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = include_bytes!("../../../contracts/native/fixtures/cursor-trajectory-v1.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        crate::protocol::trajectory::CURSOR_TRAJECTORY_FIXTURE_SHA
    );
    let fixtures: Value = serde_json::from_slice(bytes)?;
    for case in fixtures["cases"].as_array().ok_or("cases")? {
        let points: Vec<_> = case["input"]
            .as_array()
            .ok_or("input")?
            .iter()
            .map(source_point)
            .collect::<Result<_, _>>()?;
        let original = points.clone();
        let indices = retained_indices(&points).map_err(|_| "simplification failed")?;
        let actual: Vec<_> = indices
            .into_iter()
            .map(|index| points[index].observation_seq)
            .collect();
        assert_eq!(
            serde_json::to_value(actual)?,
            case["expectedRetainedObservationSeq"],
            "{}",
            case["id"]
        );
        assert_eq!(points, original);
    }
    Ok(())
}

#[test]
fn invalid_order_oversize_and_span_are_typed_refusals() {
    let point = SedObservation {
        observation_seq: 0,
        segment_id: 0,
        active_time_us: 0,
        position_q8: Some((0, 0)),
        visible: true,
        button_event: false,
        semantic_anchor: false,
    };
    assert_eq!(retained_indices(&[]), Ok(Vec::new()));
    assert_eq!(retained_indices(&[point]), Ok(vec![0]));
    assert_eq!(
        retained_indices(&[point, point]),
        Err(TrajectoryWindowError::InvalidObservation)
    );
    let mut points: Vec<_> = (0..=MAX_WINDOW_OBSERVATIONS)
        .map(|index| SedObservation {
            observation_seq: index as u64,
            ..point
        })
        .collect();
    assert_eq!(
        retained_indices(&points),
        Err(TrajectoryWindowError::TooManyObservations)
    );
    points.truncate(2);
    points[1].active_time_us = 250_001;
    assert_eq!(
        retained_indices(&points),
        Err(TrajectoryWindowError::SpanExceeded)
    );
    points[1].active_time_us = 1;
    points[1].position_q8 = Some((u64::MAX, 0));
    assert_eq!(
        retained_indices(&points),
        Err(TrajectoryWindowError::InvalidObservation)
    );
}
