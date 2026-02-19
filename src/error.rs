use thiserror::Error;

#[derive(Error, Debug)]
pub enum AiError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Thread not found: {0}")]
    ThreadNotFound(String),

    #[error("Bridge not found: {0}")]
    BridgeNotFound(String),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Message not found: {0}")]
    MessageNotFound(String),

    #[error("Project not found: {0}")]
    ProjectNotFound(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Capacity exceeded: {0}")]
    CapacityExceeded(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type AiResult<T> = Result<T, AiError>;
