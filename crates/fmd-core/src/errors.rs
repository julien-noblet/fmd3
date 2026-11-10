use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Image processing error: {0}")]
    Image(#[from] image::ImageError),

    #[error("Archive error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),

    #[error("Core error: {0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, Error>;
