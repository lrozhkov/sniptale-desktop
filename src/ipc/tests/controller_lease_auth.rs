use crate::{
    ipc::controller_lease::{ControllerLeaseBook, ControllerLeaseError},
    protocol::{
        BrowserFamily, ControllerAcquireReason, ControllerLeaseStatus, NativeControllerIdentity,
    },
};
use proptest::{prelude::*, test_runner::TestCaseError};

fn identity(connection_id: &str, profile_key: &str) -> NativeControllerIdentity {
    NativeControllerIdentity {
        extension_id: "abcdefghijklmnopqrstuvwxzyabcdef".to_owned(),
        browser_family: BrowserFamily::Chrome,
        profile_key: profile_key.to_owned(),
        document_id: Some(format!("document-{connection_id}")),
        connection_id: connection_id.to_owned(),
    }
}

fn identity_with(
    connection_id: &str,
    browser_family: BrowserFamily,
    profile_key: &str,
    document_id: Option<&str>,
) -> NativeControllerIdentity {
    NativeControllerIdentity {
        extension_id: "abcdefghijklmnopqrstuvwxzyabcdef".to_owned(),
        browser_family,
        profile_key: profile_key.to_owned(),
        document_id: document_id.map(str::to_owned),
        connection_id: connection_id.to_owned(),
    }
}

#[test]
fn first_connection_acquires_and_validates_lease() -> Result<(), ControllerLeaseError> {
    let mut book = ControllerLeaseBook::new(60_000)?;
    let controller = identity("connection-1", "profile-1");

    let decision = book.acquire(
        controller.clone(),
        &ControllerAcquireReason::InitialConnect,
        1_000,
        "lease-1",
    )?;

    assert_eq!(decision.status, ControllerLeaseStatus::Granted);
    assert_eq!(decision.granted_lease_id.as_deref(), Some("lease-1"));
    assert_eq!(decision.expires_at_epoch_ms, 61_000);
    assert!(book.validate(&controller, "lease-1", 1_000).is_ok());
    Ok(())
}

#[test]
fn second_initial_connection_does_not_receive_active_lease() -> Result<(), ControllerLeaseError> {
    let mut book = ControllerLeaseBook::new(60_000)?;
    let active = identity("connection-1", "profile-1");
    let observer = identity("connection-2", "profile-2");
    book.acquire(
        active.clone(),
        &ControllerAcquireReason::InitialConnect,
        1_000,
        "lease-1",
    )?;

    let decision = book.acquire(
        observer,
        &ControllerAcquireReason::InitialConnect,
        2_000,
        "lease-2",
    )?;

    assert_eq!(decision.status, ControllerLeaseStatus::OwnedByOtherProfile);
    assert_eq!(decision.controller, active);
    assert_eq!(decision.granted_lease_id, None);
    assert!(
        book.validate(&identity("connection-2", "profile-2"), "lease-1", 2_000)
            .is_err()
    );
    Ok(())
}

#[test]
fn browser_family_difference_is_not_the_active_owner() -> Result<(), ControllerLeaseError> {
    let mut book = ControllerLeaseBook::new(60_000)?;
    let chrome = identity_with(
        "connection-1",
        BrowserFamily::Chrome,
        "profile-1",
        Some("document-1"),
    );
    let edge = identity_with(
        "connection-2",
        BrowserFamily::Edge,
        "profile-1",
        Some("document-1"),
    );
    book.acquire(
        chrome.clone(),
        &ControllerAcquireReason::InitialConnect,
        1_000,
        "lease-1",
    )?;

    let decision = book.acquire(
        edge.clone(),
        &ControllerAcquireReason::InitialConnect,
        2_000,
        "lease-2",
    )?;

    assert_eq!(decision.status, ControllerLeaseStatus::OwnedByOtherProfile);
    assert_eq!(decision.controller, chrome);
    assert!(matches!(
        book.validate(&edge, "lease-1", 2_000),
        Err(ControllerLeaseError::NonOwner)
    ));
    Ok(())
}

#[test]
fn document_id_difference_is_not_the_active_owner() -> Result<(), ControllerLeaseError> {
    let mut book = ControllerLeaseBook::new(60_000)?;
    let first_document = identity_with(
        "connection-1",
        BrowserFamily::Chrome,
        "profile-1",
        Some("document-1"),
    );
    let second_document = identity_with(
        "connection-2",
        BrowserFamily::Chrome,
        "profile-1",
        Some("document-2"),
    );
    book.acquire(
        first_document.clone(),
        &ControllerAcquireReason::InitialConnect,
        1_000,
        "lease-1",
    )?;

    let decision = book.acquire(
        second_document.clone(),
        &ControllerAcquireReason::InitialConnect,
        2_000,
        "lease-2",
    )?;

    assert_eq!(decision.status, ControllerLeaseStatus::OwnedByOtherProfile);
    assert_eq!(decision.controller, first_document);
    assert!(matches!(
        book.validate(&second_document, "lease-1", 2_000),
        Err(ControllerLeaseError::NonOwner)
    ));
    Ok(())
}

#[test]
fn requested_takeover_rotates_lease_and_rejects_old_owner() -> Result<(), ControllerLeaseError> {
    let mut book = ControllerLeaseBook::new(60_000)?;
    let first = identity("connection-1", "profile-1");
    let second = identity("connection-2", "profile-2");
    book.acquire(
        first.clone(),
        &ControllerAcquireReason::InitialConnect,
        1_000,
        "lease-1",
    )?;

    let decision = book.acquire(
        second.clone(),
        &ControllerAcquireReason::UserRequestedTakeover,
        2_000,
        "lease-2",
    )?;

    assert_eq!(decision.status, ControllerLeaseStatus::Granted);
    assert_eq!(decision.granted_lease_id.as_deref(), Some("lease-2"));
    assert!(matches!(
        book.validate(&first, "lease-1", 2_000),
        Err(ControllerLeaseError::NonOwner)
    ));
    assert!(book.validate(&second, "lease-2", 2_000).is_ok());
    Ok(())
}

#[test]
fn stale_recovery_grants_only_after_expiry() -> Result<(), ControllerLeaseError> {
    let mut book = ControllerLeaseBook::new(10_000)?;
    let first = identity("connection-1", "profile-1");
    let second = identity("connection-2", "profile-2");
    book.acquire(
        first.clone(),
        &ControllerAcquireReason::InitialConnect,
        1_000,
        "lease-1",
    )?;

    let early = book.acquire(
        second.clone(),
        &ControllerAcquireReason::StaleControllerRecovery,
        10_999,
        "lease-2",
    )?;
    assert_eq!(early.status, ControllerLeaseStatus::OwnedByOtherProfile);

    let stale = book.acquire(
        second.clone(),
        &ControllerAcquireReason::StaleControllerRecovery,
        11_000,
        "lease-2",
    )?;
    assert_eq!(stale.status, ControllerLeaseStatus::Granted);
    assert!(book.validate(&second, "lease-2", 11_000).is_ok());
    Ok(())
}

#[test]
fn heartbeat_refresh_extends_active_lease() -> Result<(), ControllerLeaseError> {
    let mut book = ControllerLeaseBook::new(10_000)?;
    let controller = identity("connection-1", "profile-1");
    book.acquire(
        controller.clone(),
        &ControllerAcquireReason::InitialConnect,
        1_000,
        "lease-1",
    )?;

    let refreshed = book.refresh(&controller, "lease-1", 5_000)?;

    assert_eq!(refreshed.expires_at_epoch_ms, 15_000);
    assert!(book.validate(&controller, "lease-1", 14_999).is_ok());
    assert!(matches!(
        book.validate(&controller, "lease-1", 15_000),
        Err(ControllerLeaseError::StaleLease)
    ));
    Ok(())
}

proptest! {
    #[test]
    fn controller_lease_rejects_stale_or_replayed_lease_ids(ttl_ms in 1_u64..=60_000) {
        let mut book = ControllerLeaseBook::new(ttl_ms)
            .map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
        let first = identity("connection-1", "profile-1");
        let second = identity("connection-2", "profile-2");
        book.acquire(
            first.clone(),
            &ControllerAcquireReason::InitialConnect,
            1_000,
            "lease-1",
        )
        .map_err(|error| TestCaseError::fail(format!("{error:?}")))?;

        prop_assert!(book.validate(&first, "lease-1", 999 + ttl_ms).is_ok());
        prop_assert!(matches!(
            book.validate(&first, "lease-1", 1_000 + ttl_ms),
            Err(ControllerLeaseError::StaleLease)
        ));

        book.acquire(
            second.clone(),
            &ControllerAcquireReason::UserRequestedTakeover,
            2_000,
            "lease-2",
        )
        .map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
        prop_assert!(matches!(
            book.validate(&first, "lease-1", 2_000),
            Err(ControllerLeaseError::NonOwner)
        ));
        prop_assert!(book.validate(&second, "lease-2", 2_000).is_ok());
    }
}

#[test]
fn release_is_owner_scoped_and_idempotent() -> Result<(), ControllerLeaseError> {
    let mut book = ControllerLeaseBook::new(10_000)?;
    let owner = identity("connection-1", "profile-1");
    let observer = identity("connection-2", "profile-2");
    book.acquire(
        owner.clone(),
        &ControllerAcquireReason::InitialConnect,
        1_000,
        "lease-1",
    )?;

    assert!(!book.release(&observer));
    assert!(book.release(&owner));
    assert!(!book.release(&owner));
    assert!(matches!(
        book.validate(&owner, "lease-1", 2_000),
        Err(ControllerLeaseError::NoActiveLease)
    ));
    Ok(())
}
