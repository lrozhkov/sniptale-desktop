//! Redacted diagnostics bundle creation owner.

mod zip;

use std::{
    fmt, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use self::zip::{ZipEntryInput, write_zip_file};
use super::log::{DiagnosticOutput, redact_for_output};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum BundleEntryKind {
    DoctorJson,
    VersionJson,
    ManifestSummary,
    CapabilitySnapshot,
    RecentErrorSummary,
    ObservabilitySnapshot,
    RawLog,
    MediaPayload,
    TempMedia,
    TextInputBuffer,
}

const EXCLUDED_BUNDLE_ENTRY_KINDS: &[BundleEntryKind] = &[
    BundleEntryKind::RawLog,
    BundleEntryKind::MediaPayload,
    BundleEntryKind::TempMedia,
    BundleEntryKind::TextInputBuffer,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BundleCandidate {
    pub(crate) kind: BundleEntryKind,
    pub(crate) name: String,
    pub(crate) content: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BundleEntry {
    pub(crate) kind: BundleEntryKind,
    pub(crate) name: String,
    pub(crate) redacted_content: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BundlePlan {
    pub(crate) entries: Vec<BundleEntry>,
    pub(crate) excluded_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BundleWriteResult {
    pub(crate) path: PathBuf,
    pub(crate) file_name: String,
    pub(crate) entries_written: usize,
    pub(crate) excluded_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BundleWriteError {
    Clock,
    Io,
    Json,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleFile<'a> {
    format_version: u16,
    created_at_epoch_ms: u64,
    entries: Vec<BundleManifestEntry<'a>>,
    excluded_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleManifestEntry<'a> {
    kind: BundleEntryKind,
    name: &'a str,
}

pub(crate) fn plan_bundle(candidates: &[BundleCandidate]) -> BundlePlan {
    let mut entries = Vec::new();
    let mut excluded_count = 0;
    for candidate in candidates {
        if excluded_from_bundle(candidate.kind) {
            excluded_count += 1;
            continue;
        }
        entries.push(BundleEntry {
            kind: candidate.kind,
            name: sanitize_bundle_entry_name(&candidate.name),
            redacted_content: redact_for_output(
                DiagnosticOutput::SupportBundle,
                &candidate.content,
                "install-health",
                "collect-diagnostics",
            )
            .message,
        });
    }
    BundlePlan {
        entries,
        excluded_count,
    }
}

pub(crate) fn default_bundle_dir() -> PathBuf {
    std::env::temp_dir()
        .join("sniptale-native-agent")
        .join("diagnostics")
}

pub(crate) fn write_runtime_bundle(
    candidates: &[BundleCandidate],
) -> Result<BundleWriteResult, BundleWriteError> {
    let created_at_epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| BundleWriteError::Clock)?
        .as_millis();
    let created_at_epoch_ms = u64::try_from(created_at_epoch_ms).unwrap_or(u64::MAX);
    write_bundle_to_dir(&default_bundle_dir(), created_at_epoch_ms, candidates)
}

pub(crate) fn write_bundle_to_dir(
    directory: &Path,
    created_at_epoch_ms: u64,
    candidates: &[BundleCandidate],
) -> Result<BundleWriteResult, BundleWriteError> {
    let plan = plan_bundle(candidates);
    fs::create_dir_all(directory).map_err(|_| BundleWriteError::Io)?;

    let file_name = format!(
        "sniptale-diagnostics-{created_at_epoch_ms}-{}.zip",
        std::process::id()
    );
    let path = directory.join(&file_name);
    let manifest = BundleFile {
        format_version: 1,
        created_at_epoch_ms,
        entries: plan
            .entries
            .iter()
            .map(|entry| BundleManifestEntry {
                kind: entry.kind,
                name: &entry.name,
            })
            .collect(),
        excluded_count: plan.excluded_count,
    };
    let manifest = serde_json::to_vec_pretty(&manifest).map_err(|_| BundleWriteError::Json)?;
    let mut zip_entries = Vec::with_capacity(plan.entries.len().saturating_add(1));
    zip_entries.push(ZipEntryInput {
        name: "manifest.json",
        content: &manifest,
    });
    for entry in &plan.entries {
        zip_entries.push(ZipEntryInput {
            name: &entry.name,
            content: entry.redacted_content.as_bytes(),
        });
    }
    write_zip_file(&path, &zip_entries).map_err(|_| BundleWriteError::Io)?;

    Ok(BundleWriteResult {
        path,
        file_name,
        entries_written: plan.entries.len(),
        excluded_count: plan.excluded_count,
    })
}

fn excluded_from_bundle(kind: BundleEntryKind) -> bool {
    EXCLUDED_BUNDLE_ENTRY_KINDS.contains(&kind)
}

fn sanitize_bundle_entry_name(name: &str) -> String {
    if name.is_empty()
        || name.contains('\\')
        || name.contains('/')
        || name.contains(':')
        || name.contains("..")
    {
        return "redacted-entry.txt".to_owned();
    }

    let sanitized: String = name
        .chars()
        .take(96)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect();

    if sanitized.is_empty() {
        "redacted-entry.txt".to_owned()
    } else {
        sanitized
    }
}

impl Serialize for BundleEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Entry<'a> {
            kind: BundleEntryKind,
            name: &'a str,
            redacted_content: &'a str,
        }

        Entry {
            kind: self.kind,
            name: &self.name,
            redacted_content: &self.redacted_content,
        }
        .serialize(serializer)
    }
}

impl fmt::Display for BundleWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Clock => "diagnostics bundle clock error",
            Self::Io => "diagnostics bundle io error",
            Self::Json => "diagnostics bundle json error",
        })
    }
}

impl std::error::Error for BundleWriteError {}
