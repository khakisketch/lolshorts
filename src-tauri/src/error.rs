use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("File system error: {0}")]
    Io(String),
    #[error("Video processing error: {0}")]
    Video(String),
    #[error("LCU (League Client) error: {0}")]
    Lcu(String),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Resource not found: {0}")]
    NotFound(String),
    #[error("Recording error: {0}")]
    Recording(String),
    #[error("Internal system error: {0}")]
    Internal(String),
    #[error("Out of memory: {0}")]
    OutOfMemory(String),
    #[error("Process timeout: {0}")]
    ProcessTimeout(String),
    #[error("Corrupted file: {0}")]
    CorruptedFile(String),
    #[error("Device disconnected: {0}")]
    DeviceDisconnected(String),
    #[error("Rate limited: {0}")]
    RateLimited(String),
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("Updater error ({code}): {message}")]
    Updater { code: String, message: String },
}

// Serialize implementation to provide structured error response to Frontend
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let (code, message) = match self {
            AppError::Database(msg) => ("DATABASE_ERROR", msg.clone()),
            AppError::Network(msg) => ("NETWORK_ERROR", msg.clone()),
            AppError::Io(msg) => ("IO_ERROR", msg.clone()),
            AppError::Video(msg) => ("VIDEO_ERROR", msg.clone()),
            AppError::Lcu(msg) => ("LCU_ERROR", msg.clone()),
            AppError::Auth(msg) => ("AUTH_ERROR", msg.clone()),
            AppError::Validation(msg) => ("VALIDATION_ERROR", msg.clone()),
            AppError::NotFound(msg) => ("NOT_FOUND", msg.clone()),
            AppError::Recording(msg) => ("RECORDING_ERROR", msg.clone()),
            AppError::Internal(msg) => ("INTERNAL_ERROR", msg.clone()),
            AppError::OutOfMemory(msg) => ("OUT_OF_MEMORY", msg.clone()),
            AppError::ProcessTimeout(msg) => ("PROCESS_TIMEOUT", msg.clone()),
            AppError::CorruptedFile(msg) => ("CORRUPTED_FILE", msg.clone()),
            AppError::DeviceDisconnected(msg) => ("DEVICE_DISCONNECTED", msg.clone()),
            AppError::RateLimited(msg) => ("RATE_LIMITED", msg.clone()),
            AppError::ServiceUnavailable(msg) => ("SERVICE_UNAVAILABLE", msg.clone()),
            AppError::Updater { code, message } => (code.as_str(), message.clone()),
        };

        let response = ErrorResponse {
            code: code.to_string(),
            message,
        };

        response.serialize(serializer)
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    code: String,
    message: String,
}

// Helper for generic Result type
pub type AppResult<T> = Result<T, AppError>;

// Implement From<anyhow::Error> for easy conversion
impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

// Implement From<std::io::Error>
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_out_of_memory_error_serializes() {
        let err = AppError::OutOfMemory("buffer allocation failed".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "OUT_OF_MEMORY");
        assert_eq!(json["message"], "buffer allocation failed");
    }

    #[test]
    fn test_process_timeout_error_serializes() {
        let err = AppError::ProcessTimeout("FFmpeg exceeded 10min".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "PROCESS_TIMEOUT");
        assert_eq!(json["message"], "FFmpeg exceeded 10min");
    }

    #[test]
    fn test_corrupted_file_error_serializes() {
        let err = AppError::CorruptedFile("segment_003.mp4".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "CORRUPTED_FILE");
        assert_eq!(json["message"], "segment_003.mp4");
    }

    #[test]
    fn test_device_disconnected_error_serializes() {
        let err = AppError::DeviceDisconnected("USB Headset".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "DEVICE_DISCONNECTED");
        assert_eq!(json["message"], "USB Headset");
    }

    #[test]
    fn test_rate_limited_error_serializes() {
        let err = AppError::RateLimited("recording commands".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "RATE_LIMITED");
        assert_eq!(json["message"], "recording commands");
    }

    #[test]
    fn test_service_unavailable_error_serializes() {
        let err = AppError::ServiceUnavailable("LCU API".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "SERVICE_UNAVAILABLE");
        assert_eq!(json["message"], "LCU API");
    }

    #[test]
    fn test_all_variants_have_code_and_message() {
        let variants: Vec<AppError> = vec![
            AppError::Database("test".into()),
            AppError::Network("test".into()),
            AppError::Io("test".into()),
            AppError::Video("test".into()),
            AppError::Auth("test".into()),
            AppError::Validation("test".into()),
            AppError::NotFound("test".into()),
            AppError::Recording("test".into()),
            AppError::Internal("test".into()),
            AppError::Lcu("test".into()),
            AppError::OutOfMemory("test".into()),
            AppError::ProcessTimeout("test".into()),
            AppError::CorruptedFile("test".into()),
            AppError::DeviceDisconnected("test".into()),
            AppError::RateLimited("test".into()),
            AppError::ServiceUnavailable("test".into()),
            AppError::Updater {
                code: "update_check_failed".into(),
                message: "test".into(),
            },
        ];
        for err in &variants {
            let json = serde_json::to_value(err).unwrap();
            assert!(json["code"].is_string(), "Missing code for {:?}", err);
            assert!(json["message"].is_string(), "Missing message for {:?}", err);
        }
    }
}
