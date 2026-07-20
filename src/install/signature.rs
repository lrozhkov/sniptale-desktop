//! Installed binary signature verification owner.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SignatureStatus {
    #[cfg(any(windows, test))]
    Valid,
    #[cfg(any(windows, test))]
    Missing,
    #[cfg(any(windows, test))]
    Invalid,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SignaturePolicy {
    Required,
    #[cfg(any(windows, test))]
    BestEffortDev,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SignatureError {
    #[cfg(any(windows, test))]
    RequiredSignatureMissing,
    #[cfg(any(windows, test))]
    InvalidSignature,
    UnknownSignatureState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BinarySignatureReport {
    pub(crate) status: SignatureStatus,
    pub(crate) signer_subject: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SignatureProbeResult {
    #[cfg(any(windows, test))]
    Valid,
    #[cfg(any(windows, test))]
    Missing,
    #[cfg(any(windows, test))]
    Invalid,
    Unknown,
}

pub(crate) fn current_binary_signature_report(path: &str) -> BinarySignatureReport {
    signature_report_from_probe(current_binary_signature_probe(path))
}

pub(crate) fn signature_report_from_probe(probe: SignatureProbeResult) -> BinarySignatureReport {
    BinarySignatureReport {
        status: match probe {
            #[cfg(any(windows, test))]
            SignatureProbeResult::Valid => SignatureStatus::Valid,
            #[cfg(any(windows, test))]
            SignatureProbeResult::Missing => SignatureStatus::Missing,
            #[cfg(any(windows, test))]
            SignatureProbeResult::Invalid => SignatureStatus::Invalid,
            SignatureProbeResult::Unknown => SignatureStatus::Unknown,
        },
        signer_subject: None,
    }
}

pub(crate) fn evaluate_signature(
    report: &BinarySignatureReport,
    policy: SignaturePolicy,
) -> Result<(), SignatureError> {
    match (report.status, policy) {
        #[cfg(any(windows, test))]
        (SignatureStatus::Valid, _) => Ok(()),
        #[cfg(any(windows, test))]
        (SignatureStatus::Missing, SignaturePolicy::Required) => {
            Err(SignatureError::RequiredSignatureMissing)
        }
        #[cfg(any(windows, test))]
        (SignatureStatus::Invalid, _) => Err(SignatureError::InvalidSignature),
        (SignatureStatus::Unknown, _) => Err(SignatureError::UnknownSignatureState),
        #[cfg(any(windows, test))]
        (SignatureStatus::Missing, SignaturePolicy::BestEffortDev) => Ok(()),
    }
}

#[cfg(windows)]
fn current_binary_signature_probe(path: &str) -> SignatureProbeResult {
    super::windows::signature::verify_file_signature(path)
}

#[cfg(not(windows))]
fn current_binary_signature_probe(_path: &str) -> SignatureProbeResult {
    SignatureProbeResult::Unknown
}
