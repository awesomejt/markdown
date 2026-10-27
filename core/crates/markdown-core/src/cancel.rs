//! Cooperative cancellation shared by parsing and document sessions.
use crate::ParseError;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cloneable, thread-safe cancellation flag. Cancelling is idempotent and
/// irreversible; every clone observes it. Polling never blocks or allocates.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}
impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }
    /// `Err(Cancelled)` once [`cancel`](Self::cancel) has been observed.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Marker returned when a cancellation token was observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;
impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}
impl std::error::Error for Cancelled {}

/// Outcome of a cancellable parse: cooperative cancellation or a structural
/// [`ParseError`]. Neither variant carries partial output or document contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CancellableParseError {
    Cancelled,
    Parse(ParseError),
}
impl From<ParseError> for CancellableParseError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}
impl From<Cancelled> for CancellableParseError {
    fn from(_: Cancelled) -> Self {
        Self::Cancelled
    }
}
impl std::fmt::Display for CancellableParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Parse(error) => write!(f, "{error}"),
        }
    }
}
impl std::error::Error for CancellableParseError {}
