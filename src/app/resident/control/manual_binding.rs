//! Creates a frozen Manual authority, local route, and durable job before media work starts.

use crate::{
    assets::{AssetFormat, AssetJob, prepare_manual_asset_job},
    protocol::{DeliveryRoute, NativeRecordingSource, OperationAuthority, VideoMode},
    recording::{RecordingDeliverySession, RecordingStartPolicy, RecordingStartRequest},
    settings::{ActiveSettings, ModeEpoch, OperatingMode},
    transfer::identity::derived_media_id,
};

#[derive(Clone, Debug)]
pub(crate) struct ManualOperationBinding {
    pub(crate) epoch: ModeEpoch,
    pub(crate) authority: OperationAuthority,
    pub(crate) delivery_route: DeliveryRoute,
    pub(crate) job: AssetJob,
}

pub(crate) fn bind_manual_recording(
    settings: &ActiveSettings,
    operation_id: &str,
    mode: VideoMode,
    requested_at_epoch_ms: u64,
) -> Result<
    (
        ManualOperationBinding,
        RecordingStartRequest,
        RecordingStartPolicy,
    ),
    ManualOperationBindError,
> {
    let binding = bind_manual_operation(settings, operation_id, AssetFormat::Mp4)?;
    let epoch = settings
        .epoch()
        .map_err(|_| ManualOperationBindError::ModeUnavailable)?;
    let profile = settings
        .profile_snapshot()
        .map_err(|_| ManualOperationBindError::ModeUnavailable)?;
    let revision = profile.revision().to_string();
    let preferences = profile
        .manual_recording_settings()
        .ok_or(ManualOperationBindError::ModeUnavailable)?;
    if epoch != binding.epoch {
        return Err(ManualOperationBindError::StaleEpoch);
    }
    let request = RecordingStartRequest {
        operation_id: operation_id.to_owned(),
        source: NativeRecordingSource {
            mode,
            display_id: None,
            window_id: None,
            region: None,
            microphone_device_id: preferences.microphone_device_id,
        },
        authority: binding.authority.clone(),
        delivery_route: binding.delivery_route.clone(),
        settings_revision: revision,
        requested_quality: preferences.requested_quality,
        include_cursor: preferences.include_cursor,
        open_editor: false,
        requested_at_epoch_ms,
        delivery_session: RecordingDeliverySession::default(),
    };
    Ok((
        binding,
        request,
        RecordingStartPolicy {
            max_duration_ms: preferences.max_duration_ms,
            telemetry: crate::telemetry::RecordingTelemetryPolicy::default(),
        },
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManualOperationBindError {
    ModeUnavailable,
    StaleEpoch,
    DestinationUnavailable,
}

pub(crate) fn bind_manual_operation(
    settings: &ActiveSettings,
    operation_id: &str,
    format: AssetFormat,
) -> Result<ManualOperationBinding, ManualOperationBindError> {
    let epoch = settings
        .epoch()
        .map_err(|_| ManualOperationBindError::ModeUnavailable)?;
    if epoch.stable_mode() != Some(OperatingMode::Manual) {
        return Err(ManualOperationBindError::ModeUnavailable);
    }
    let profile = settings
        .profile_snapshot()
        .map_err(|_| ManualOperationBindError::ModeUnavailable)?;
    let destination = profile
        .manual_destination()
        .ok_or(ManualOperationBindError::DestinationUnavailable)?;
    let rechecked = settings
        .epoch()
        .map_err(|_| ManualOperationBindError::ModeUnavailable)?;
    if rechecked != epoch {
        return Err(ManualOperationBindError::StaleEpoch);
    }
    let asset_job_id = derived_media_id("asset", operation_id);
    let job = prepare_manual_asset_job(
        asset_job_id.clone(),
        format,
        destination.id,
        destination.root,
    )
    .map_err(|_| ManualOperationBindError::DestinationUnavailable)?;
    Ok(ManualOperationBinding {
        epoch,
        authority: OperationAuthority::Manual {
            profile_revision: profile.revision().to_string(),
        },
        delivery_route: DeliveryRoute::LocalAsset { asset_job_id },
        job,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{ManualOperationBindError, bind_manual_operation, bind_manual_recording};
    use crate::{
        assets::AssetFormat,
        protocol::{DeliveryRoute, OperationAuthority, VideoMode},
        settings::{
            ActiveSettings, LocalProfileDraft, ManualProfileDraft,
            apply_local_profile_draft_from_path, ensure_local_profile_from_path,
        },
    };

    #[test]
    fn binding_freezes_manual_authority_route_and_prepared_job()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempdir()?;
        let profile_path = directory.path().join("local-profile.json");
        let profile = ensure_local_profile_from_path(Some(profile_path.clone()))
            .map_err(|_| std::io::Error::other("profile"))?;
        let settings = ActiveSettings::from_profile(&profile);
        let draft = LocalProfileDraft::manual(ManualProfileDraft::with_defaults(
            "destination-1".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ));
        apply_local_profile_draft_from_path(Some(profile_path), &settings, &draft)
            .map_err(|_| std::io::Error::other("apply"))?;

        let binding = bind_manual_operation(&settings, "operation-1", AssetFormat::Png)
            .map_err(|_| std::io::Error::other("binding"))?;
        assert!(matches!(
            binding.authority,
            OperationAuthority::Manual { .. }
        ));
        assert!(matches!(
            binding.delivery_route,
            DeliveryRoute::LocalAsset { .. }
        ));
        assert_eq!(binding.epoch.generation(), 2);
        assert!(matches!(
            binding.job.state(),
            crate::assets::AssetJobState::Prepared
        ));
        Ok(())
    }

    #[test]
    fn extension_mode_cannot_create_a_local_asset_binding() -> Result<(), Box<dyn std::error::Error>>
    {
        let profile =
            ensure_local_profile_from_path(None).map_err(|_| std::io::Error::other("profile"))?;
        let settings = ActiveSettings::from_profile(&profile);
        assert!(matches!(
            bind_manual_operation(&settings, "operation-1", AssetFormat::Png),
            Err(ManualOperationBindError::ModeUnavailable)
        ));
        Ok(())
    }

    #[test]
    fn recording_binding_freezes_profile_quality_and_local_delivery()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempdir()?;
        let profile_path = directory.path().join("local-profile.json");
        let profile = ensure_local_profile_from_path(Some(profile_path.clone()))
            .map_err(|_| std::io::Error::other("profile"))?;
        let settings = ActiveSettings::from_profile(&profile);
        let draft = LocalProfileDraft::manual(ManualProfileDraft::with_defaults(
            "destination-1".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ));
        apply_local_profile_draft_from_path(Some(profile_path), &settings, &draft)
            .map_err(|_| std::io::Error::other("apply"))?;

        let (binding, request, policy) =
            bind_manual_recording(&settings, "recording-operation-1", VideoMode::Screen, 4_000)
                .map_err(|_| std::io::Error::other("binding"))?;

        assert_eq!(request.delivery_session.controller_lease_id, None);
        assert_eq!(request.settings_revision, "2");
        assert_eq!(request.requested_quality.audio_bitrate_kbps, 128);
        assert_eq!(policy.max_duration_ms, 3_600_000);
        assert_eq!(binding.epoch.generation(), 2);
        Ok(())
    }
}
