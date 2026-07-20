use super::{
    PROTECTED_MEDIA_ALGORITHM, TempMediaRecord, TempMediaState, TempStoreError,
    progress::map_lifecycle_store_error, validate_media_id, validate_protected_media_key,
    validate_recording_metadata, validate_sha256, validate_size,
};

impl TempMediaRecord {
    pub(crate) fn validate_for_persistence(&self) -> Result<(), TempStoreError> {
        validate_media_id(&self.media_id)?;
        if self.updated_at_ms < self.created_at_ms {
            return Err(TempStoreError::ClockMovedBackwards);
        }
        self.validate_route_snapshot()?;
        match self.state {
            TempMediaState::Writing => self.validate_writing_shape(),
            TempMediaState::Finalized | TempMediaState::Acked | TempMediaState::Rejected => {
                self.validate_finalized_shape()?;
                if let Some(recording) = &self.recording {
                    validate_recording_metadata(recording)?;
                }
                Ok(())
            }
        }
    }

    fn validate_writing_shape(&self) -> Result<(), TempStoreError> {
        if self.total_bytes.is_none()
            && self.sha256.is_none()
            && self.retain_rejected_until_ms.is_none()
            && self.next_missing_chunk_index == 0
            && self.media_protection.is_none()
            && self.recording.is_none()
        {
            Ok(())
        } else {
            Err(TempStoreError::InvalidTransition)
        }
    }

    fn validate_route_snapshot(&self) -> Result<(), TempStoreError> {
        match (
            self.route_schema,
            self.authority.as_ref(),
            self.delivery_route.as_ref(),
        ) {
            (None, None, None)
            | (
                Some(1),
                Some(crate::protocol::OperationAuthority::ExtensionManaged { .. }),
                Some(crate::protocol::DeliveryRoute::ExtensionTransfer),
            ) => Ok(()),
            (
                Some(1),
                Some(crate::protocol::OperationAuthority::Manual { .. }),
                Some(crate::protocol::DeliveryRoute::LocalAsset { asset_job_id }),
            ) if !asset_job_id.is_empty() => Ok(()),
            _ => Err(TempStoreError::InvalidTransition),
        }
    }

    fn validate_finalized_shape(&self) -> Result<(), TempStoreError> {
        let total_bytes = self.total_bytes.ok_or(TempStoreError::InvalidSize)?;
        validate_size(total_bytes)?;
        self.lifecycle_snapshot()
            .validate_finalized_progress()
            .map_err(|error| map_lifecycle_store_error(&error))?;
        validate_sha256(
            self.sha256
                .as_deref()
                .ok_or(TempStoreError::InvalidSha256)?,
        )?;
        self.lifecycle_snapshot()
            .validate_rejected_retention_window()
            .map_err(|error| map_lifecycle_store_error(&error))?;
        if let Some(protection) = &self.media_protection {
            validate_protected_media_key(protection, PROTECTED_MEDIA_ALGORITHM)
                .map_err(|_| TempStoreError::InvalidSha256)?;
        }
        Ok(())
    }
}
