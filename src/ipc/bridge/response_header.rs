//! Resident agent response-batch header.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

pub(crate) const AGENT_PIPE_RESPONSE_HEADER_MAX_BYTES: usize = 256;
const AGENT_PIPE_MAX_RESPONSES_PER_REQUEST: usize = 2_048;

#[derive(Debug)]
pub(crate) enum AgentPipeResponseHeaderError {
    TooManyResponses { count: usize, limit: usize },
    Json(serde_json::Error),
}

impl fmt::Display for AgentPipeResponseHeaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyResponses { count, limit } => {
                write!(formatter, "{count} responses exceeds {limit}")
            }
            Self::Json(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for AgentPipeResponseHeaderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::TooManyResponses { .. } => None,
            Self::Json(error) => Some(error),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentPipeResponseHeader {
    response_count: u32,
}

pub(crate) fn agent_pipe_response_header_payload(
    response_count: usize,
) -> Result<Vec<u8>, AgentPipeResponseHeaderError> {
    validate_agent_response_count(response_count)?;
    let response_count =
        u32::try_from(response_count).map_err(|_| response_count_too_large(response_count))?;
    serde_json::to_vec(&AgentPipeResponseHeader { response_count })
        .map_err(AgentPipeResponseHeaderError::Json)
}

#[cfg(any(windows, test))]
pub(super) fn parse_agent_pipe_response_header(
    payload: &[u8],
) -> Result<usize, AgentPipeResponseHeaderError> {
    let header: AgentPipeResponseHeader =
        serde_json::from_slice(payload).map_err(AgentPipeResponseHeaderError::Json)?;
    let response_count =
        usize::try_from(header.response_count).map_err(|_| response_count_too_large(usize::MAX))?;
    validate_agent_response_count(response_count)?;
    Ok(response_count)
}

fn validate_agent_response_count(
    response_count: usize,
) -> Result<(), AgentPipeResponseHeaderError> {
    if response_count > AGENT_PIPE_MAX_RESPONSES_PER_REQUEST {
        Err(response_count_too_large(response_count))
    } else {
        Ok(())
    }
}

fn response_count_too_large(count: usize) -> AgentPipeResponseHeaderError {
    AgentPipeResponseHeaderError::TooManyResponses {
        count,
        limit: AGENT_PIPE_MAX_RESPONSES_PER_REQUEST,
    }
}
