//! Authenticated consent storage bound to the current installation's pipe credential.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

const MAX_BYTES: usize = 128 * 1024;
const DOMAIN: &[u8] = b"sniptale-persisted-consent-v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PersistedTrustError {
    Invalid,
    Oversized,
    Authentication,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    version: u16,
    payload: String,
    authentication_tag: Vec<u8>,
}

pub(crate) fn seal_persisted_trust(
    payload: &[u8],
    secret: &[u8],
) -> Result<Vec<u8>, PersistedTrustError> {
    if payload.len() > MAX_BYTES / 2 {
        return Err(PersistedTrustError::Oversized);
    }
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret).map_err(|_| PersistedTrustError::Authentication)?;
    mac.update(DOMAIN);
    mac.update(payload);
    let envelope = Envelope {
        version: 1,
        payload: String::from_utf8(payload.to_vec()).map_err(|_| PersistedTrustError::Invalid)?,
        authentication_tag: mac.finalize().into_bytes().to_vec(),
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|_| PersistedTrustError::Invalid)?;
    if bytes.len() > MAX_BYTES {
        return Err(PersistedTrustError::Oversized);
    }
    Ok(bytes)
}

pub(crate) fn open_persisted_trust(
    bytes: &[u8],
    secret: &[u8],
) -> Result<Vec<u8>, PersistedTrustError> {
    if bytes.len() > MAX_BYTES {
        return Err(PersistedTrustError::Oversized);
    }
    let envelope: Envelope =
        serde_json::from_slice(bytes).map_err(|_| PersistedTrustError::Invalid)?;
    if envelope.version != 1
        || envelope.authentication_tag.len() != 32
        || envelope.payload.len() > MAX_BYTES / 2
    {
        return Err(PersistedTrustError::Invalid);
    }
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret).map_err(|_| PersistedTrustError::Authentication)?;
    mac.update(DOMAIN);
    mac.update(envelope.payload.as_bytes());
    mac.verify_slice(&envelope.authentication_tag)
        .map_err(|_| PersistedTrustError::Authentication)?;
    Ok(envelope.payload.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persisted_consent_survives_restart_but_rejects_forgery_reinstall_and_oversize()
    -> Result<(), Box<dyn std::error::Error>> {
        let payload = br#"{"trusted":[]}"#;
        let bytes = seal_persisted_trust(payload, b"installation-one").map_err(|_| "seal")?;
        assert_eq!(
            open_persisted_trust(&bytes, b"installation-one").map_err(|_| "open")?,
            payload
        );
        assert_eq!(
            open_persisted_trust(&bytes, b"installation-two"),
            Err(PersistedTrustError::Authentication)
        );
        let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
        value["payload"] = serde_json::json!("forged");
        assert_eq!(
            open_persisted_trust(&serde_json::to_vec(&value)?, b"installation-one"),
            Err(PersistedTrustError::Authentication)
        );
        assert!(open_persisted_trust(payload, b"installation-one").is_err());
        assert_eq!(
            open_persisted_trust(&vec![b' '; MAX_BYTES + 1], b"key"),
            Err(PersistedTrustError::Oversized)
        );
        Ok(())
    }
}
