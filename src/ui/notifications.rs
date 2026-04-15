use std::time::Instant;

/// A transient notification shown as a floating overlay.
#[derive(Clone)]
pub struct Toast {
    pub message: String,
    pub is_error: bool,
    pub created_at: Instant,
}

impl Toast {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: false,
            created_at: Instant::now(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: true,
            created_at: Instant::now(),
        }
    }
}