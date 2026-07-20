use std::error::Error;

use proptest::{prelude::*, test_runner::TestCaseError};
use serde::Deserialize;

use super::super::{
    bundle::{BundleCandidate, BundleEntryKind, plan_bundle},
    doctor::{DoctorCheckName, DoctorStatus, doctor_check, doctor_report},
    log::{
        DiagnosticOutput,
        crash::{CrashContext, build_crash_log_record},
        record::{
            LogErrorFields, LogLevel, StructuredLogContext, build_structured_log_record,
            serialize_log_record_json_line,
        },
        redact_for_output,
    },
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    samples: Vec<Sample>,
    required_outputs: Vec<String>,
}

#[derive(Deserialize)]
struct Sample {
    value: String,
}

fn corpus() -> Result<Corpus, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../../contracts/security/redaction-corpus.json"
    ))
}

fn output(name: &str) -> DiagnosticOutput {
    match name {
        "log" => DiagnosticOutput::Log,
        "doctor-json" => DiagnosticOutput::DoctorJson,
        "collect-diagnostics" => DiagnosticOutput::CollectDiagnostics,
        "operation-failure" => DiagnosticOutput::OperationFailure,
        _ => DiagnosticOutput::SupportBundle,
    }
}

#[test]
fn corpus_is_redacted_for_required_outputs() -> Result<(), Box<dyn Error>> {
    let corpus = corpus()?;
    for required_output in &corpus.required_outputs {
        for sample in &corpus.samples {
            let diagnostic = redact_for_output(
                output(required_output),
                &sample.value,
                "recording",
                "transfer",
            );

            assert!(!diagnostic.message.contains(&sample.value));
            assert!(!diagnostic.message.contains("secret-token"));
            assert!(
                !diagnostic
                    .message
                    .contains("https://example.invalid/private")
            );
            assert!(!diagnostic.message.contains("AAAA_SECRET_MEDIA_BYTES"));
            assert!(!diagnostic.message.contains("password123"));
            assert!(!diagnostic.message.contains("copied-secret"));
            assert!(!diagnostic.message.contains("Authorization: Bearer"));
            assert!(!diagnostic.message.contains("C:\\Users\\Alice\\Private"));
            assert!(
                !diagnostic
                    .message
                    .contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            );
            assert!(!diagnostic.message.contains("recording-secret-9"));
            assert!(!diagnostic.message.contains("screenshot-secret-9"));
            assert!(!diagnostic.message.contains("capture-secret-9"));
            assert_eq!(diagnostic.operation, "recording");
            assert_eq!(diagnostic.phase, "transfer");
            assert!(
                diagnostic.message.contains("redacted")
                    || diagnostic.message.contains("path-category")
                    || diagnostic.message.contains("message-type")
            );
        }
    }
    Ok(())
}

proptest! {
    #[test]
    fn redaction_corpus_required_outputs_cover_fuzz_sensitive_categories(
        sample_index in 0_usize..64,
        output_index in 0_usize..8
    ) {
        let corpus = corpus().map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert!(!corpus.samples.is_empty());
        prop_assert!(!corpus.required_outputs.is_empty());
        let sample = &corpus.samples[sample_index % corpus.samples.len()];
        let required_output = &corpus.required_outputs[output_index % corpus.required_outputs.len()];

        let diagnostic = redact_for_output(
            output(required_output),
            &sample.value,
            "recording",
            "transfer",
        );

        prop_assert!(!diagnostic.message.contains(&sample.value));
        prop_assert!(!diagnostic.message.contains("secret-token"));
        prop_assert!(!diagnostic.message.contains("AAAA_SECRET_MEDIA_BYTES"));
        prop_assert!(!diagnostic.message.contains("password123"));
        prop_assert!(!diagnostic.message.contains("copied-secret"));
        prop_assert!(!diagnostic.message.contains("Authorization: Bearer"));
        prop_assert!(!diagnostic.message.contains("C:\\Users\\Alice\\Private"));
    }
}

#[test]
fn operation_failure_payload_projection_redacts_correlation_fields() {
    let payload = serde_json::json!({
        "type": "app.operation.failed",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "operation": "recording",
        "operationId": "recording-secret-9",
        "phase": "transfer-state",
        "error": {
            "code": "storage-failed",
            "message": "pending media transfer state is unavailable",
            "recoverable": true
        },
        "occurredAtEpochMs": 1
    });
    let diagnostic = redact_for_output(
        DiagnosticOutput::OperationFailure,
        &payload.to_string(),
        "operation-failure",
        "payload",
    );

    assert_eq!(diagnostic.output, DiagnosticOutput::OperationFailure);
    assert_eq!(diagnostic.operation, "operation-failure");
    assert_eq!(diagnostic.phase, "payload");
    assert!(
        diagnostic
            .message
            .contains("message-type=app.operation.failed")
    );
    assert!(diagnostic.message.contains("operation=recording"));
    assert!(diagnostic.message.contains("phase=transfer-state"));
    assert!(diagnostic.message.contains("error-code=storage-failed"));
    assert!(!diagnostic.message.contains("lease:connection-1"));
    assert!(!diagnostic.message.contains("recording-secret-9"));
    assert!(!diagnostic.message.contains("pending media transfer"));
}

#[test]
fn raw_native_payload_keeps_message_type_category_only() {
    let diagnostic = redact_for_output(
        DiagnosticOutput::OperationFailure,
        "{\"type\":\"extension.recording.start\",\"token\":\"secret-token\"}",
        "recording",
        "parse",
    );

    assert!(
        diagnostic
            .message
            .contains("message-type=extension.recording.start")
    );
    assert!(!diagnostic.message.contains("secret-token"));
    assert!(!diagnostic.message.contains("{\"type\""));
}

#[test]
fn malformed_native_payload_does_not_echo_raw_type_value() {
    let diagnostic = redact_for_output(
        DiagnosticOutput::Log,
        "{\"type\":\"extension.recording.start token secret-token\"",
        "recording",
        "parse",
    );

    assert!(diagnostic.message.contains("message-type=unknown"));
    assert!(!diagnostic.message.contains("secret-token"));
}

#[test]
fn corpus_is_redacted_by_generated_diagnostics_outputs() -> Result<(), Box<dyn Error>> {
    let corpus = corpus()?;
    for sample in &corpus.samples {
        let outputs = [
            generated_log_line(&sample.value)?,
            generated_doctor_json(&sample.value)?,
            generated_support_bundle_plan(&sample.value)?,
            generated_operation_failure_projection(&sample.value),
            generated_crash_log_line(&sample.value)?,
        ];
        for output in outputs {
            assert_forbidden_absent(&output);
            assert!(
                output.contains("redacted")
                    || output.contains("path-category")
                    || output.contains("message-type")
                    || output.contains("storage-failed")
            );
        }
    }
    Ok(())
}

fn generated_log_line(input: &str) -> Result<String, serde_json::Error> {
    let record = build_structured_log_record(
        1_000,
        LogLevel::Warn,
        "sniptale_native_agent::diagnostics",
        log_context(),
        input,
        Some(12),
        Some(LogErrorFields {
            code: "storage-failed".to_owned(),
            recoverable: true,
        }),
    );
    serialize_log_record_json_line(&record)
}

fn generated_doctor_json(input: &str) -> Result<String, serde_json::Error> {
    serde_json::to_string(&doctor_report(vec![doctor_check(
        DoctorCheckName::RuntimeStartup,
        DoctorStatus::Fail,
        input,
    )]))
}

fn generated_support_bundle_plan(input: &str) -> Result<String, serde_json::Error> {
    let plan = plan_bundle(&[BundleCandidate {
        kind: BundleEntryKind::DoctorJson,
        name: "doctor.json".to_owned(),
        content: input.to_owned(),
    }]);
    serde_json::to_string(&serde_json::json!({
        "entries": plan.entries,
        "excludedCount": plan.excluded_count,
    }))
}

fn generated_operation_failure_projection(input: &str) -> String {
    let payload = serde_json::json!({
        "type": "app.operation.failed",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "operation": "recording",
        "operationId": "recording-secret-9",
        "phase": "transfer-state",
        "error": {
            "code": "storage-failed",
            "message": input,
            "recoverable": true
        },
        "occurredAtEpochMs": 1
    });
    redact_for_output(
        DiagnosticOutput::OperationFailure,
        &payload.to_string(),
        "operation-failure",
        "payload",
    )
    .message
}

fn generated_crash_log_line(input: &str) -> Result<String, serde_json::Error> {
    let record = build_crash_log_record(
        2_000,
        &CrashContext::with_phase(
            "native-host",
            "recording",
            Some("recording-secret-9".to_owned()),
            "finalizing",
        ),
        input,
    );
    serialize_log_record_json_line(&record)
}

fn log_context() -> StructuredLogContext {
    StructuredLogContext {
        app_version: "0.1.0".to_owned(),
        protocol_version: 1,
        settings_schema_version: 1,
        app_cache_schema_version: 1,
        platform_kind: "windows".to_owned(),
        process_mode: "collect-diagnostics".to_owned(),
        process_id: 42,
        connection_id: Some("connection-1".to_owned()),
        controller_lease_id: Some("lease:connection-1".to_owned()),
        operation: "diagnostics",
        operation_id: Some("capture-secret-9".to_owned()),
        phase: "support-bundle",
    }
}

fn assert_forbidden_absent(output: &str) {
    for forbidden in [
        "secret-token",
        "https://example.invalid/private",
        "AAAA_SECRET_MEDIA_BYTES",
        "password123",
        "copied-secret",
        "Authorization: Bearer",
        "Cookie: session=secret",
        "C:\\Users\\Alice\\Private",
        "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "recording-secret-9",
        "screenshot-secret-9",
        "capture-secret-9",
        "lease:connection-1",
        "connection-1",
    ] {
        assert!(
            !output.contains(forbidden),
            "generated diagnostics output leaked {forbidden}: {output}"
        );
    }
}
