use serde::Serialize;
use sleuth_store::StoreError;

/// Stable, machine-readable error categories (exposed to the UI and MCP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    InvalidInput,
    Cancelled,
    Io,
    Internal,
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("cancelled")]
    Cancelled,
    #[error("io error: {0}")]
    Io(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl ServiceError {
    pub fn code(&self) -> ErrorCode {
        match self {
            ServiceError::NotFound(_) => ErrorCode::NotFound,
            ServiceError::InvalidInput(_) => ErrorCode::InvalidInput,
            ServiceError::Cancelled => ErrorCode::Cancelled,
            ServiceError::Io(_) => ErrorCode::Io,
            ServiceError::Internal(_) => ErrorCode::Internal,
        }
    }
}

/// Serialized as `{ "code": "...", "message": "..." }` for the UI.
impl Serialize for ServiceError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("ServiceError", 2)?;
        st.serialize_field("code", &self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

impl From<StoreError> for ServiceError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound(m) => ServiceError::NotFound(m),
            StoreError::Invalid(m) => ServiceError::InvalidInput(m),
            StoreError::Io(e) => ServiceError::Io(e.to_string()),
            other => ServiceError::Internal(other.to_string()),
        }
    }
}

impl From<std::io::Error> for ServiceError {
    fn from(e: std::io::Error) -> Self {
        ServiceError::Io(e.to_string())
    }
}
