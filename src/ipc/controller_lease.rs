//! Active controller ownership and stale lease handling.

use crate::protocol::{ControllerAcquireReason, ControllerLeaseStatus, NativeControllerIdentity};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ControllerLeaseError {
    #[cfg(test)]
    InvalidTtl,
    InvalidLeaseId,
    ExpiryOverflow,
    NoActiveLease,
    StaleLease,
    LeaseMismatch,
    NonOwner,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ControllerLeaseDecision {
    pub(crate) status: ControllerLeaseStatus,
    pub(crate) controller: NativeControllerIdentity,
    pub(crate) granted_lease_id: Option<String>,
    pub(crate) expires_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActiveControllerLease {
    controller: NativeControllerIdentity,
    lease_id: String,
    expires_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ControllerLeaseBook {
    ttl_ms: u64,
    active: Option<ActiveControllerLease>,
}

impl Default for ControllerLeaseBook {
    fn default() -> Self {
        Self {
            ttl_ms: 300_000,
            active: None,
        }
    }
}

impl ControllerLeaseBook {
    #[cfg(test)]
    pub(crate) fn new(ttl_ms: u64) -> Result<Self, ControllerLeaseError> {
        if ttl_ms == 0 {
            return Err(ControllerLeaseError::InvalidTtl);
        }
        Ok(Self {
            ttl_ms,
            active: None,
        })
    }

    pub(crate) fn acquire(
        &mut self,
        controller: NativeControllerIdentity,
        reason: &ControllerAcquireReason,
        requested_at_epoch_ms: u64,
        new_lease_id: impl Into<String>,
    ) -> Result<ControllerLeaseDecision, ControllerLeaseError> {
        let new_lease_id = validate_lease_id(new_lease_id.into())?;
        if self.should_grant(&controller, reason, requested_at_epoch_ms) {
            let expires_at_epoch_ms = self.expires_at(requested_at_epoch_ms)?;
            self.active = Some(ActiveControllerLease {
                controller: controller.clone(),
                lease_id: new_lease_id.clone(),
                expires_at_epoch_ms,
            });
            Ok(ControllerLeaseDecision {
                status: ControllerLeaseStatus::Granted,
                controller,
                granted_lease_id: Some(new_lease_id),
                expires_at_epoch_ms,
            })
        } else {
            let active = self
                .active
                .as_ref()
                .ok_or(ControllerLeaseError::NoActiveLease)?;
            Ok(ControllerLeaseDecision {
                status: ControllerLeaseStatus::OwnedByOtherProfile,
                controller: active.controller.clone(),
                granted_lease_id: None,
                expires_at_epoch_ms: active.expires_at_epoch_ms,
            })
        }
    }

    #[cfg(test)]
    pub(crate) fn refresh(
        &mut self,
        controller: &NativeControllerIdentity,
        lease_id: &str,
        now_epoch_ms: u64,
    ) -> Result<ControllerLeaseDecision, ControllerLeaseError> {
        self.validate(controller, lease_id, now_epoch_ms)?;
        let expires_at_epoch_ms = self.expires_at(now_epoch_ms)?;
        let active = self
            .active
            .as_mut()
            .ok_or(ControllerLeaseError::NoActiveLease)?;
        active.expires_at_epoch_ms = expires_at_epoch_ms;
        Ok(ControllerLeaseDecision {
            status: ControllerLeaseStatus::Granted,
            controller: active.controller.clone(),
            granted_lease_id: Some(active.lease_id.clone()),
            expires_at_epoch_ms,
        })
    }

    pub(crate) fn validate(
        &self,
        controller: &NativeControllerIdentity,
        lease_id: &str,
        now_epoch_ms: u64,
    ) -> Result<(), ControllerLeaseError> {
        let active = self
            .active
            .as_ref()
            .ok_or(ControllerLeaseError::NoActiveLease)?;
        if now_epoch_ms >= active.expires_at_epoch_ms {
            return Err(ControllerLeaseError::StaleLease);
        }
        if &active.controller != controller {
            return Err(ControllerLeaseError::NonOwner);
        }
        if active.lease_id != lease_id {
            return Err(ControllerLeaseError::LeaseMismatch);
        }
        Ok(())
    }

    #[cfg(any(windows, test))]
    pub(crate) fn release(&mut self, controller: &NativeControllerIdentity) -> bool {
        let should_release = self
            .active
            .as_ref()
            .is_some_and(|active| &active.controller == controller);
        if should_release {
            self.active = None;
        }
        should_release
    }

    fn should_grant(
        &self,
        controller: &NativeControllerIdentity,
        reason: &ControllerAcquireReason,
        requested_at_epoch_ms: u64,
    ) -> bool {
        match &self.active {
            None => true,
            Some(active) if active.is_expired(requested_at_epoch_ms) => true,
            Some(active) if &active.controller == controller => true,
            Some(_) if reason == &ControllerAcquireReason::UserRequestedTakeover => true,
            Some(_) => false,
        }
    }

    fn expires_at(&self, now_epoch_ms: u64) -> Result<u64, ControllerLeaseError> {
        now_epoch_ms
            .checked_add(self.ttl_ms)
            .ok_or(ControllerLeaseError::ExpiryOverflow)
    }
}

impl ActiveControllerLease {
    const fn is_expired(&self, now_epoch_ms: u64) -> bool {
        now_epoch_ms >= self.expires_at_epoch_ms
    }
}

fn validate_lease_id(lease_id: String) -> Result<String, ControllerLeaseError> {
    if lease_id.is_empty() {
        Err(ControllerLeaseError::InvalidLeaseId)
    } else {
        Ok(lease_id)
    }
}
