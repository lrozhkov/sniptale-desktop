use crate::transfer::trajectory_crypto::{
    TrajectoryAuthentication, TrajectoryCipher, TrajectoryCryptoError,
};

#[test]
fn encrypted_blocks_bind_sequence_length_identity_and_wrapped_key()
-> Result<(), TrajectoryCryptoError> {
    let trajectory = "0123456789abcdef0123456789abcdef";
    let (cipher, protected) = TrajectoryCipher::create("recording-test", trajectory)?;
    let plaintext = b"private cursor block";
    let sealed = cipher.seal(0, plaintext)?;
    let other = cipher.seal(0, plaintext)?;
    assert_ne!(sealed.nonce, other.nonce);
    assert!(
        !sealed
            .ciphertext
            .windows(plaintext.len())
            .any(|part| part == plaintext)
    );
    let opened = TrajectoryCipher::open("recording-test", trajectory, &protected)?;
    assert_eq!(
        opened.unseal(0, plaintext.len(), &sealed.nonce, &sealed.ciphertext)?,
        plaintext
    );
    assert!(
        opened
            .unseal(1, plaintext.len(), &sealed.nonce, &sealed.ciphertext)
            .is_err()
    );
    assert!(
        opened
            .unseal(0, plaintext.len() + 1, &sealed.nonce, &sealed.ciphertext)
            .is_err()
    );
    let mut corrupt = sealed.ciphertext.clone();
    corrupt[0] ^= 1;
    assert!(
        opened
            .unseal(0, plaintext.len(), &sealed.nonce, &corrupt)
            .is_err()
    );
    for (recording, id) in [
        ("other-recording", trajectory),
        ("recording-test", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    ] {
        let substituted = TrajectoryCipher::open(recording, id, &protected)?;
        assert!(
            substituted
                .unseal(0, plaintext.len(), &sealed.nonce, &sealed.ciphertext)
                .is_err()
        );
    }
    assert!(cipher.seal(0, &[]).is_err());
    assert!(cipher.seal(0, &vec![0; 65537]).is_err());
    let maximum = cipher.seal(7, &vec![1; 65536])?;
    assert_eq!(
        cipher
            .unseal(7, 65536, &maximum.nonce, &maximum.ciphertext)?
            .len(),
        65536
    );
    Ok(())
}

#[test]
fn ledger_and_index_authentication_have_separate_domains() -> Result<(), TrajectoryCryptoError> {
    let (cipher, _) =
        TrajectoryCipher::create("recording-test", "0123456789abcdef0123456789abcdef")?;
    let bytes = b"committed head";
    let tag = cipher.authenticate(TrajectoryAuthentication::Ledger, bytes)?;
    cipher.verify(TrajectoryAuthentication::Ledger, bytes, &tag)?;
    assert!(
        cipher
            .verify(TrajectoryAuthentication::Index, bytes, &tag)
            .is_err()
    );
    assert!(
        cipher
            .verify(TrajectoryAuthentication::Ledger, b"different head", &tag)
            .is_err()
    );
    let mut corrupt = tag;
    corrupt[31] ^= 1;
    assert!(
        cipher
            .verify(TrajectoryAuthentication::Ledger, bytes, &corrupt)
            .is_err()
    );
    assert!(
        cipher
            .verify(TrajectoryAuthentication::Ledger, bytes, &tag[..31])
            .is_err()
    );
    Ok(())
}
