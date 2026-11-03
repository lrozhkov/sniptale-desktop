//! Durable browser selection, separate from the immutable capture route policy.

use crate::protocol::DeliveryRoute;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum BrowserMediaKind {
    Screenshot,
    Recording,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserDeliveryError {
    Invalid,
    Io,
    WrongTarget,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RecordingHistoryBinding {
    #[serde(deserialize_with = "required_history_id")]
    pub(crate) cursor_trajectory_id: Option<String>,
    #[serde(deserialize_with = "required_history_id")]
    pub(crate) event_stream_id: Option<String>,
}

fn required_history_id<'de, D: serde::Deserializer<'de>>(
    input: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(input)
}

impl RecordingHistoryBinding {
    fn valid(&self) -> bool {
        self.cursor_trajectory_id
            .iter()
            .chain(self.event_stream_id.iter())
            .all(|id| crate::protocol::trajectory::valid_trajectory_id(id))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Selection {
    version: u16,
    media_id: String,
    kind: BrowserMediaKind,
    target_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    histories: Option<RecordingHistoryBinding>,
}

#[derive(Clone, Debug)]
pub(crate) struct BrowserDeliveryStore {
    root: PathBuf,
}

impl BrowserDeliveryStore {
    pub(crate) const fn open(root: PathBuf) -> Self {
        Self { root }
    }
    pub(crate) fn open_default() -> Result<Self, BrowserDeliveryError> {
        let root = std::env::var_os("LOCALAPPDATA").ok_or(BrowserDeliveryError::Io)?;
        Ok(Self::open(
            PathBuf::from(root)
                .join("Sniptale")
                .join("NativeAgent")
                .join("browser-delivery"),
        ))
    }
    pub(crate) fn select(
        &self,
        media_id: &str,
        kind: BrowserMediaKind,
        route: &DeliveryRoute,
        target_id: &str,
    ) -> Result<(), BrowserDeliveryError> {
        if !valid_target_id(target_id) {
            return Err(BrowserDeliveryError::Invalid);
        }
        if !matches!(route, DeliveryRoute::DesktopDelivery { target_id: expected, ask_after_capture, .. } if *ask_after_capture || expected.as_deref() == Some(target_id))
        {
            return Err(BrowserDeliveryError::WrongTarget);
        }
        let path = self.path(media_id, kind)?;
        if let Some(existing) = self.target(media_id, kind)? {
            return if existing == target_id {
                Ok(())
            } else {
                Err(BrowserDeliveryError::WrongTarget)
            };
        }
        fs::create_dir_all(&self.root).map_err(|_| BrowserDeliveryError::Io)?;
        let selection = Selection {
            version: 1,
            media_id: media_id.to_owned(),
            kind,
            target_id: target_id.to_owned(),
            histories: None,
        };
        self.write_selection(&selection, &path)
    }

    fn write_selection(
        &self,
        selection: &Selection,
        path: &std::path::Path,
    ) -> Result<(), BrowserDeliveryError> {
        if path.parent() != Some(self.root.as_path()) {
            return Err(BrowserDeliveryError::Invalid);
        }
        let payload = serde_json::to_vec(selection).map_err(|_| BrowserDeliveryError::Invalid)?;
        let pending = path.with_extension("pending");
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&pending)
            .map_err(|_| BrowserDeliveryError::Io)?;
        let written = file.write_all(&payload).and_then(|()| file.sync_all());
        drop(file);
        if written.is_err() || fs::rename(&pending, path).is_err() {
            let _ = fs::remove_file(pending);
            return Err(BrowserDeliveryError::Io);
        }
        #[cfg(not(windows))]
        fs::File::open(&self.root)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| BrowserDeliveryError::Io)?;
        Ok(())
    }

    pub(crate) fn target(
        &self,
        media_id: &str,
        kind: BrowserMediaKind,
    ) -> Result<Option<String>, BrowserDeliveryError> {
        Ok(self
            .read_selection(media_id, kind)?
            .map(|selection| selection.target_id))
    }

    fn read_selection(
        &self,
        media_id: &str,
        kind: BrowserMediaKind,
    ) -> Result<Option<Selection>, BrowserDeliveryError> {
        let file = match fs::File::open(self.path(media_id, kind)?) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(BrowserDeliveryError::Io),
        };
        let mut bytes = Vec::new();
        file.take(4097)
            .read_to_end(&mut bytes)
            .map_err(|_| BrowserDeliveryError::Io)?;
        if bytes.len() > 4096 {
            return Err(BrowserDeliveryError::Invalid);
        }
        let selection: Selection =
            serde_json::from_slice(&bytes).map_err(|_| BrowserDeliveryError::Invalid)?;
        if selection.version != 1
            || selection.media_id != media_id
            || selection.kind != kind
            || !valid_target_id(&selection.target_id)
            || selection
                .histories
                .as_ref()
                .is_some_and(|binding| kind != BrowserMediaKind::Recording || !binding.valid())
        {
            return Err(BrowserDeliveryError::Invalid);
        }
        Ok(Some(selection))
    }
    pub(crate) fn bind_recording_histories(
        &self,
        media_id: &str,
        target: &str,
        histories: RecordingHistoryBinding,
    ) -> Result<(), BrowserDeliveryError> {
        if !histories.valid() {
            return Err(BrowserDeliveryError::Invalid);
        }
        let mut selection = self
            .read_selection(media_id, BrowserMediaKind::Recording)?
            .ok_or(BrowserDeliveryError::WrongTarget)?;
        if selection.target_id != target {
            return Err(BrowserDeliveryError::WrongTarget);
        }
        if let Some(existing) = selection.histories {
            return if existing == histories {
                Ok(())
            } else {
                Err(BrowserDeliveryError::Invalid)
            };
        }
        selection.histories = Some(histories);
        self.write_selection(
            &selection,
            &self.path(media_id, BrowserMediaKind::Recording)?,
        )
    }

    pub(crate) fn recording_histories(
        &self,
        media_id: &str,
        target: &str,
    ) -> Result<Option<RecordingHistoryBinding>, BrowserDeliveryError> {
        let Some(selection) = self.read_selection(media_id, BrowserMediaKind::Recording)? else {
            return Ok(None);
        };
        if selection.target_id != target {
            return Err(BrowserDeliveryError::WrongTarget);
        }
        Ok(selection.histories)
    }

    /// Authorizes the immutable capture route against the exact durable media selection.
    pub(crate) fn authorize_route(
        &self,
        media_id: &str,
        kind: BrowserMediaKind,
        route: &DeliveryRoute,
        target_id: &str,
    ) -> Result<(), BrowserDeliveryError> {
        if !matches!(route, DeliveryRoute::DesktopDelivery { target_id: expected, ask_after_capture, .. }
            if *ask_after_capture || expected.as_deref() == Some(target_id))
            || self.target(media_id, kind)?.as_deref() != Some(target_id)
        {
            return Err(BrowserDeliveryError::WrongTarget);
        }
        Ok(())
    }

    /// Called only after verified publication or durable ACK has released the source.
    pub(crate) fn remove_completed(
        &self,
        media_id: &str,
        kind: BrowserMediaKind,
    ) -> Result<(), BrowserDeliveryError> {
        match fs::remove_file(self.path(media_id, kind)?) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(BrowserDeliveryError::Io),
        }
    }

    fn path(
        &self,
        media_id: &str,
        kind: BrowserMediaKind,
    ) -> Result<PathBuf, BrowserDeliveryError> {
        if media_id.is_empty()
            || media_id.len() > 128
            || !media_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(BrowserDeliveryError::Invalid);
        }
        let prefix = match kind {
            BrowserMediaKind::Screenshot => "screenshot",
            BrowserMediaKind::Recording => "recording",
        };
        Ok(self.root.join(format!("{prefix}-{media_id}.json")))
    }
}

pub(crate) fn valid_target_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attachment_set_is_exact_immutable_and_owned_by_the_selected_browser()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let store = BrowserDeliveryStore::open(directory.path().to_owned());
        let target = "a".repeat(64);
        let route = DeliveryRoute::DesktopDelivery {
            asset_job_id: "history-job".to_owned(),
            target_id: Some(target.clone()),
            ask_after_capture: false,
        };
        let binding = RecordingHistoryBinding {
            cursor_trajectory_id: Some("a".repeat(32)),
            event_stream_id: Some("b".repeat(32)),
        };
        assert_eq!(
            store.bind_recording_histories("history", &target, binding.clone()),
            Err(BrowserDeliveryError::WrongTarget)
        );
        store
            .select("history", BrowserMediaKind::Recording, &route, &target)
            .map_err(|error| format!("{error:?}"))?;
        store
            .bind_recording_histories("history", &target, binding.clone())
            .map_err(|error| format!("{error:?}"))?;
        let path = store
            .path("history", BrowserMediaKind::Recording)
            .map_err(|error| format!("{error:?}"))?;
        let committed = fs::read(&path)?;
        store
            .bind_recording_histories("history", &target, binding.clone())
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(fs::read(&path)?, committed);
        let mut changed = binding.clone();
        changed.event_stream_id = None;
        assert_eq!(
            store.bind_recording_histories("history", &target, changed),
            Err(BrowserDeliveryError::Invalid)
        );
        assert_eq!(
            store.recording_histories("history", &"b".repeat(64)),
            Err(BrowserDeliveryError::WrongTarget)
        );
        assert_eq!(
            store.recording_histories("history", &target),
            Ok(Some(binding))
        );
        assert_eq!(fs::read(&path)?, committed);
        let mut broken: serde_json::Value = serde_json::from_slice(&committed)?;
        broken["histories"]
            .as_object_mut()
            .ok_or("binding")?
            .remove("eventStreamId");
        let bytes = serde_json::to_vec(&broken)?;
        fs::write(&path, &bytes)?;
        assert_eq!(
            store.recording_histories("history", &target),
            Err(BrowserDeliveryError::Invalid)
        );
        assert_eq!(fs::read(&path)?, bytes);
        Ok(())
    }
    #[test]
    fn selection_rejects_wrong_target_retargeting_paths_and_oversize()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let store = BrowserDeliveryStore::open(directory.path().to_owned());
        let first = "a".repeat(64);
        let second = "b".repeat(64);
        let route = DeliveryRoute::DesktopDelivery {
            asset_job_id: "job".to_owned(),
            target_id: Some(first.clone()),
            ask_after_capture: false,
        };
        assert_eq!(
            store.select("media-one", BrowserMediaKind::Screenshot, &route, &second),
            Err(BrowserDeliveryError::WrongTarget)
        );
        store
            .select("media-one", BrowserMediaKind::Screenshot, &route, &first)
            .map_err(|_| "select")?;
        assert_eq!(
            BrowserDeliveryStore::open(directory.path().to_owned())
                .target("media-one", BrowserMediaKind::Screenshot)
                .map_err(|_| "read")?,
            Some(first)
        );
        let ask = DeliveryRoute::DesktopDelivery {
            asset_job_id: "job".to_owned(),
            target_id: None,
            ask_after_capture: true,
        };
        assert_eq!(
            store.select("media-one", BrowserMediaKind::Screenshot, &ask, &second),
            Err(BrowserDeliveryError::WrongTarget)
        );
        assert!(
            store
                .target("../escape", BrowserMediaKind::Screenshot)
                .is_err()
        );
        std::fs::write(
            directory.path().join("recording-bad.json"),
            vec![b' '; 4097],
        )?;
        assert!(store.target("bad", BrowserMediaKind::Recording).is_err());
        Ok(())
    }
    #[test]
    fn selection_is_idempotent_and_corrupt_or_unwritable_state_fails_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let store = BrowserDeliveryStore::open(directory.path().to_owned());
        let id = "a".repeat(64);
        let route = DeliveryRoute::DesktopDelivery {
            asset_job_id: "job".to_owned(),
            target_id: None,
            ask_after_capture: true,
        };
        for invalid in ["", "bad-target", &"A".repeat(64)] {
            assert_eq!(
                store.select("one", BrowserMediaKind::Recording, &route, invalid),
                Err(BrowserDeliveryError::Invalid)
            );
        }
        store
            .select("one", BrowserMediaKind::Recording, &route, &id)
            .map_err(|_| "select")?;
        let path = directory.path().join("recording-one.json");
        let saved = fs::read(&path)?;
        store
            .select("one", BrowserMediaKind::Recording, &route, &id)
            .map_err(|_| "idempotent")?;
        assert_eq!(fs::read(&path)?, saved);
        for malformed in [
            b"not-json".to_vec(),
            serde_json::to_vec(&Selection {
                version: 2,
                media_id: "one".to_owned(),
                kind: BrowserMediaKind::Recording,
                target_id: id.clone(),
                histories: None,
            })?,
            serde_json::to_vec(&Selection {
                version: 1,
                media_id: "wrong".to_owned(),
                kind: BrowserMediaKind::Recording,
                target_id: id.clone(),
                histories: None,
            })?,
        ] {
            fs::write(&path, &malformed)?;
            assert_eq!(
                store.target("one", BrowserMediaKind::Recording),
                Err(BrowserDeliveryError::Invalid)
            );
            assert_eq!(fs::read(&path)?, malformed);
        }
        fs::write(
            directory.path().join("recording-two.pending"),
            b"owned-by-another-writer",
        )?;
        assert_eq!(
            store.select("two", BrowserMediaKind::Recording, &route, &id),
            Err(BrowserDeliveryError::Io)
        );
        assert_eq!(
            fs::read(directory.path().join("recording-two.pending"))?,
            b"owned-by-another-writer"
        );
        let blocked = directory.path().join("blocked");
        fs::write(&blocked, b"file")?;
        let unavailable = BrowserDeliveryStore::open(blocked.join("nested"));
        assert_eq!(
            unavailable.select("one", BrowserMediaKind::Recording, &route, &id),
            Err(BrowserDeliveryError::Io)
        );
        Ok(())
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    #[test]
    fn completed_assignment_cleanup_is_exact_idempotent_and_preserves_other_media()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let store = BrowserDeliveryStore::open(directory.path().to_owned());
        let route = DeliveryRoute::DesktopDelivery {
            asset_job_id: "job".to_owned(),
            target_id: None,
            ask_after_capture: true,
        };
        let target = "a".repeat(64);
        store
            .select("same", BrowserMediaKind::Screenshot, &route, &target)
            .map_err(|_| "screenshot binding")?;
        store
            .select("same", BrowserMediaKind::Recording, &route, &target)
            .map_err(|_| "recording binding")?;
        store
            .select("other", BrowserMediaKind::Screenshot, &route, &target)
            .map_err(|_| "other binding")?;
        assert_eq!(
            store.remove_completed("../outside", BrowserMediaKind::Screenshot),
            Err(BrowserDeliveryError::Invalid)
        );
        store
            .remove_completed("same", BrowserMediaKind::Screenshot)
            .map_err(|_| "cleanup")?;
        store
            .remove_completed("same", BrowserMediaKind::Screenshot)
            .map_err(|_| "idempotent cleanup")?;
        assert_eq!(store.target("same", BrowserMediaKind::Screenshot), Ok(None));
        assert_eq!(
            store.target("same", BrowserMediaKind::Recording),
            Ok(Some(target.clone()))
        );
        assert_eq!(
            store.target("other", BrowserMediaKind::Screenshot),
            Ok(Some(target))
        );
        Ok(())
    }
}
