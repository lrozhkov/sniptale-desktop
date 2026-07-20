//! Versioned resident pipe authentication MAC.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::server::{PipeAuthError, PipeChallenge, PipeSecret};

pub(super) const PIPE_AUTH_ALGORITHM: &str = "pipe-auth-v2-hmac-sha256";

type PipeAuthMac = Hmac<Sha256>;

#[cfg(any(windows, test))]
pub(super) fn challenge_response_hash(
    secret: &PipeSecret,
    install_id: &str,
    challenge: &PipeChallenge,
) -> Result<String, PipeAuthError> {
    Ok(digest_to_hex(
        challenge_mac(secret, install_id, challenge)?
            .finalize()
            .into_bytes(),
    ))
}

pub(super) fn verify_challenge_response_hash(
    secret: &PipeSecret,
    install_id: &str,
    challenge: &PipeChallenge,
    response_hash: &str,
) -> Result<(), PipeAuthError> {
    let response = hex_to_32(response_hash)?;
    challenge_mac(secret, install_id, challenge)?
        .verify_slice(&response)
        .map_err(|_| PipeAuthError::InvalidResponse)
}

fn challenge_mac(
    secret: &PipeSecret,
    install_id: &str,
    challenge: &PipeChallenge,
) -> Result<PipeAuthMac, PipeAuthError> {
    if challenge.algorithm != PIPE_AUTH_ALGORITHM {
        return Err(PipeAuthError::InvalidResponse);
    }
    let mut mac = PipeAuthMac::new_from_slice(secret.material().as_bytes())
        .map_err(|_| PipeAuthError::InvalidResponse)?;
    for part in [
        PIPE_AUTH_ALGORITHM,
        install_id,
        &challenge.session_id,
        &challenge.client_nonce,
        &challenge.server_nonce,
        &challenge.expires_at_ms.to_string(),
    ] {
        mac.update(part.as_bytes());
        mac.update(&[0]);
    }
    Ok(mac)
}

#[cfg(any(windows, test))]
fn digest_to_hex(digest: impl IntoIterator<Item = u8>) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest {
        output.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        output.push(char::from(b"0123456789abcdef"[usize::from(byte & 0x0f)]));
    }
    output
}

fn hex_to_32(hex: &str) -> Result<[u8; 32], PipeAuthError> {
    if hex.len() != 64 {
        return Err(PipeAuthError::InvalidResponse);
    }
    let mut output = [0_u8; 32];
    let bytes = hex.as_bytes();
    for index in 0..32 {
        let high = hex_value(bytes[index * 2])?;
        let low = hex_value(bytes[index * 2 + 1])?;
        output[index] = (high << 4) | low;
    }
    Ok(output)
}

fn hex_value(byte: u8) -> Result<u8, PipeAuthError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(PipeAuthError::InvalidResponse),
    }
}
