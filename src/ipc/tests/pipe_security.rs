use crate::ipc::{PipeSecurityDescriptorError, current_user_pipe_sddl};

#[test]
fn current_user_pipe_sddl_grants_only_expected_user() -> Result<(), PipeSecurityDescriptorError> {
    assert_eq!(
        current_user_pipe_sddl("S-1-5-21-1000")?,
        "D:P(A;;GA;;;S-1-5-21-1000)"
    );
    Ok(())
}

#[test]
fn current_user_pipe_sddl_rejects_sid_injection() {
    for sid in [
        "",
        "S-1-5-21-admin",
        "S-1-5-21-1000)(A;;GA;;;WD",
        "S-1-5-21-1000;",
    ] {
        assert_eq!(
            current_user_pipe_sddl(sid),
            Err(PipeSecurityDescriptorError::InvalidUserSid)
        );
    }
}
