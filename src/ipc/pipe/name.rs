//! Named-pipe endpoint naming owner.

const PIPE_PREFIX: &str = r"\\.\pipe\sniptale-native-agent-";
const AGENT_SINGLETON_SUFFIX: &str = "-agent-singleton";
const INSTALL_ID_MIN_LEN: usize = 8;
const INSTALL_ID_MAX_LEN: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PipeNameError {
    InvalidInstallId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PipeEndpointName {
    path: String,
}

impl PipeEndpointName {
    pub(crate) fn for_install(install_id: &str) -> Result<Self, PipeNameError> {
        validate_install_id(install_id)?;
        Ok(Self {
            path: format!("{PIPE_PREFIX}{install_id}"),
        })
    }

    #[cfg(any(windows, test))]
    pub(crate) fn for_agent_singleton(install_id: &str) -> Result<Self, PipeNameError> {
        validate_install_id(install_id)?;
        Ok(Self {
            path: format!("{PIPE_PREFIX}{install_id}{AGENT_SINGLETON_SUFFIX}"),
        })
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.path
    }
}

fn validate_install_id(install_id: &str) -> Result<(), PipeNameError> {
    if !(INSTALL_ID_MIN_LEN..=INSTALL_ID_MAX_LEN).contains(&install_id.len()) {
        return Err(PipeNameError::InvalidInstallId);
    }
    if install_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(())
    } else {
        Err(PipeNameError::InvalidInstallId)
    }
}
