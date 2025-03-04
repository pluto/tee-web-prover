#[derive(Debug, thiserror::Error)]
pub enum NotaryServerError {
    #[error("Error occurred from reading certificates: {0}")]
    CertificateError(String),

    #[error("Error occurred from reasing server config: {0}")]
    ServerConfigError(String),
}
