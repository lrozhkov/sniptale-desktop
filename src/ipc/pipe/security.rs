//! Named-pipe security descriptor planning.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PipeSecurityDescriptorError {
    InvalidUserSid,
}

pub(crate) fn current_user_pipe_sddl(
    current_user_sid: &str,
) -> Result<String, PipeSecurityDescriptorError> {
    validate_user_sid(current_user_sid)?;
    Ok(format!("D:P(A;;GA;;;{current_user_sid})"))
}

fn validate_user_sid(sid: &str) -> Result<(), PipeSecurityDescriptorError> {
    if sid.len() > 128 {
        return Err(PipeSecurityDescriptorError::InvalidUserSid);
    }
    let Some(rest) = sid.strip_prefix("S-1-") else {
        return Err(PipeSecurityDescriptorError::InvalidUserSid);
    };
    if !rest.is_empty()
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-')
    {
        Ok(())
    } else {
        Err(PipeSecurityDescriptorError::InvalidUserSid)
    }
}
