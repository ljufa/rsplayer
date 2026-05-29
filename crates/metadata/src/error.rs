use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("invalid argument: {0}")]
    Invalid(String),
}

pub type RepoResult<T> = Result<T, RepoError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_format_round_trips() {
        let err = RepoError::Decode("malformed bytes".to_owned());
        assert_eq!(format!("{err}"), "decode error: malformed bytes");
    }

    #[test]
    fn auto_converts_to_anyhow() {
        // Verifies the trait wiring callers depend on: `?` from RepoResult into anyhow::Result.
        let err: anyhow::Error = RepoError::Storage("disk full".to_owned()).into();
        assert!(err.to_string().contains("disk full"));
    }
}
