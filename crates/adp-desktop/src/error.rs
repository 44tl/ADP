use thiserror::Error;

#[derive(Error, Debug)]
pub enum DesktopError {
    #[error("tauri error: {0}")]
    Tauri(String),
    #[error("state error: {0}")]
    State(String),
    #[error(transparent)]
    Core(#[from] adp_core::AdpError),
    #[error(transparent)]
    Runtime(#[from] adp_runtime::RuntimeError),
}

pub type Result<T> = std::result::Result<T, DesktopError>;
