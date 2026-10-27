//! Document sessions: immutable shared snapshots, monotonically increasing
//! generations, latest-wins publication, cooperative cancellation, external
//! change state and scroll anchors.
//!
//! A session performs no file I/O and starts no threads. The platform adapter
//! owns file access, watching and scheduling: it reads bytes under its own
//! permission model, calls [`DocumentSession::submit`] and runs the returned
//! [`Job`] on a worker of its choosing. Only the newest generation can publish;
//! superseded or cancelled work is discarded without a partial snapshot.
//! See `specs/session/README.md` for the full contract.
use crate::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};

mod anchor;
pub use anchor::{ANCHOR_VERSION, Anchor, AnchorError, AnchorMatch, BlockKind, ResolvedAnchor};

/// Monotonically increasing per-session sequence number. Generations are
/// assigned to submissions and availability changes; they are not dense
/// publication counters and are meaningless across sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Generation(u64);
impl Generation {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Semantic model produced by a session; fixed when the session is created.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelVersion {
    V1,
    #[cfg(feature = "model-v2")]
    V2,
}

/// Session configuration. Limits apply to every generation.
#[derive(Clone, Copy, Debug)]
pub struct SessionOptions {
    pub model: ModelVersion,
    pub limits: ParseLimits,
}
impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            model: ModelVersion::V1,
            limits: ParseLimits::default(),
        }
    }
}

/// Maximum adapter revision-token length in bytes.
pub const MAX_REVISION_TOKEN_BYTES: usize = 256;

/// Opaque adapter-supplied content identity (for example a size/mtime/file-ID
/// or provider version tuple). The core only compares tokens for equality; it
/// never interprets them. Tokens must not contain paths or document contents.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RevisionToken(Vec<u8>);
impl RevisionToken {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, SessionError> {
        let bytes = bytes.into();
        if bytes.is_empty() || bytes.len() > MAX_REVISION_TOKEN_BYTES {
            return Err(SessionError::RevisionToken);
        }
        Ok(Self(bytes))
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
impl std::fmt::Debug for RevisionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RevisionToken({} bytes)", self.0.len())
    }
}

/// Why the platform could not read the document. Adapters map OS errors here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnreadableReason {
    PermissionDenied,
    AccessRevoked,
    Busy,
    Io,
}

/// Last availability reported by the adapter or implied by a submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Available,
    Deleted,
    Unreadable(UnreadableReason),
}

/// A platform change notification. The session never touches the filesystem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalChange {
    /// Content may have changed. With a token equal to the last accepted
    /// submission's token the notice is deduplicated without a read.
    Modified {
        token: Option<RevisionToken>,
    },
    Deleted,
    Unreadable(UnreadableReason),
}

/// What the adapter should do after reporting an [`ExternalChange`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeDisposition {
    /// Known content; no read or submission is needed.
    Unchanged,
    /// Read fresh bytes and [`submit`](DocumentSession::submit) them.
    ReadRequired,
    /// The token matches the newest, still-running submission at this
    /// generation; no read is needed and that job is not superseded.
    Pending(Generation),
    /// Availability change recorded at this generation; in-flight work was
    /// superseded. The last published snapshot remains readable but stale.
    /// Repeating an already-recorded state with nothing in flight returns the
    /// existing generation without consuming a new one.
    Recorded(Generation),
}

/// Typed session failures. None carries document contents or paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionError {
    InvalidDocumentId,
    RevisionToken,
    GenerationExhausted,
    InvalidUtf8,
    Parse(ParseError),
}
impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "parse: {error}"),
            Self::InvalidDocumentId => f.write_str("invalid_document_id"),
            Self::RevisionToken => f.write_str("revision_token"),
            Self::GenerationExhausted => f.write_str("generation_exhausted"),
            Self::InvalidUtf8 => f.write_str("invalid_utf8"),
        }
    }
}
impl std::error::Error for SessionError {}

/// The validated model held by a snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotModel {
    V1(Document),
    #[cfg(feature = "model-v2")]
    V2(crate::v2::Document),
}

/// Immutable published revision. Shared through `Arc`; it stays valid after
/// newer generations publish and after the session is dropped.
pub struct Snapshot {
    generation: Generation,
    source: Arc<str>,
    model: SnapshotModel,
    blocks: OnceLock<anchor::BlockIndex>,
}
impl Snapshot {
    pub fn generation(&self) -> Generation {
        self.generation
    }
    /// The exact UTF-8 source all spans in this snapshot refer to.
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn shared_source(&self) -> Arc<str> {
        Arc::clone(&self.source)
    }
    pub fn model(&self) -> &SnapshotModel {
        &self.model
    }
    pub fn v1(&self) -> Option<&Document> {
        match &self.model {
            SnapshotModel::V1(d) => Some(d),
            #[cfg(feature = "model-v2")]
            SnapshotModel::V2(_) => None,
        }
    }
    #[cfg(feature = "model-v2")]
    pub fn v2(&self) -> Option<&crate::v2::Document> {
        match &self.model {
            SnapshotModel::V2(d) => Some(d),
            SnapshotModel::V1(_) => None,
        }
    }
    pub fn source_info(&self) -> &SourceInfo {
        match &self.model {
            SnapshotModel::V1(d) => &d.input().source,
            #[cfg(feature = "model-v2")]
            SnapshotModel::V2(d) => &d.input().source,
        }
    }
    pub fn revision_id(&self) -> &RevisionId {
        match &self.model {
            SnapshotModel::V1(d) => &d.input().revision_id,
            #[cfg(feature = "model-v2")]
            SnapshotModel::V2(d) => &d.input().revision_id,
        }
    }
    fn structure(&self) -> (&[NodeId], &[Node]) {
        match &self.model {
            SnapshotModel::V1(d) => (&d.input().roots, &d.input().nodes),
            #[cfg(feature = "model-v2")]
            SnapshotModel::V2(d) => (&d.input().roots, &d.input().nodes),
        }
    }
    fn blocks(&self) -> &anchor::BlockIndex {
        self.blocks.get_or_init(|| {
            let (roots, nodes) = self.structure();
            anchor::BlockIndex::new(&self.source, roots, nodes)
        })
    }
}
// Debug never prints document contents.
impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snapshot")
            .field("generation", &self.generation)
            .field("source_bytes", &self.source.len())
            .finish_non_exhaustive()
    }
}

/// Point-in-time copy of session state.
#[derive(Clone, Debug)]
pub struct SessionStatus {
    /// Newest generation assigned to a submission or availability change.
    pub latest: Option<Generation>,
    /// Newest successfully published snapshot, possibly stale.
    pub published: Option<Arc<Snapshot>>,
    /// Newest generation whose content the published snapshot is known to
    /// represent (its own generation, or a later deduplicated submission).
    pub confirmed: Option<Generation>,
    pub availability: Availability,
    /// The latest submission, while it is neither resolved nor cancelled
    /// (by the session, its own token or supersession).
    pub pending: Option<Generation>,
    /// Failure of the newest resolved submission, cleared by a later success.
    pub failure: Option<(Generation, SessionError)>,
}
impl SessionStatus {
    /// True when the published snapshot reflects the newest known state.
    pub fn is_current(&self) -> bool {
        self.published.is_some()
            && self.latest == self.confirmed
            && self.availability == Availability::Available
    }
}

/// Result of running a [`Job`].
#[derive(Clone, Debug)]
pub enum Outcome {
    /// A new snapshot was published for this generation.
    Published(Arc<Snapshot>),
    /// Bytes equal the published snapshot's source; nothing was reparsed and
    /// the existing snapshot is confirmed for this generation.
    Unchanged(Arc<Snapshot>),
    /// A newer generation exists; the result was discarded.
    Superseded {
        generation: Generation,
        latest: Generation,
    },
    /// The job's token was cancelled; nothing was published.
    Cancelled { generation: Generation },
    /// The newest generation failed; the previous snapshot remains published.
    Failed {
        generation: Generation,
        error: SessionError,
    },
    /// The session was dropped before publication.
    Closed { generation: Generation },
}

struct Shared {
    document_id: DocumentId,
    options: SessionOptions,
    state: Mutex<State>,
}
struct Inflight {
    generation: u64,
    cancel: CancellationToken,
    token: Option<RevisionToken>,
}
struct State {
    last: u64,
    inflight: Option<Inflight>,
    published: Option<Arc<Snapshot>>,
    confirmed: u64,
    availability: Availability,
    accepted: Option<RevisionToken>,
    failure: Option<(Generation, SessionError)>,
}
impl State {
    fn bump(&mut self) -> Result<u64, SessionError> {
        let next = self
            .last
            .checked_add(1)
            .ok_or(SessionError::GenerationExhausted)?;
        self.last = next;
        if let Some(inflight) = self.inflight.take() {
            inflight.cancel.cancel();
        }
        Ok(next)
    }
    fn clear_inflight(&mut self, generation: u64) {
        if self
            .inflight
            .as_ref()
            .is_some_and(|i| i.generation == generation)
        {
            self.inflight = None;
        }
    }
}
fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    // No code panics while holding the lock; recover rather than propagate.
    state.lock().unwrap_or_else(PoisonError::into_inner)
}
impl Drop for Shared {
    fn drop(&mut self) {
        let state = self.state.get_mut().unwrap_or_else(PoisonError::into_inner);
        if let Some(inflight) = state.inflight.take() {
            inflight.cancel.cancel();
        }
    }
}

/// One document lineage. Dropping the session cancels in-flight work; held
/// snapshots stay valid. Share it across threads with `Arc` if needed.
pub struct DocumentSession {
    shared: Arc<Shared>,
}
impl DocumentSession {
    pub fn new(document_id: DocumentId, options: SessionOptions) -> Result<Self, SessionError> {
        if !crate::validate::id_valid(&document_id.0) {
            return Err(SessionError::InvalidDocumentId);
        }
        Ok(Self {
            shared: Arc::new(Shared {
                document_id,
                options,
                state: Mutex::new(State {
                    last: 0,
                    inflight: None,
                    published: None,
                    confirmed: 0,
                    availability: Availability::Available,
                    accepted: None,
                    failure: None,
                }),
            }),
        })
    }
    pub fn document_id(&self) -> &DocumentId {
        &self.shared.document_id
    }
    pub fn options(&self) -> SessionOptions {
        self.shared.options
    }
    /// Assign the next generation to adapter-read bytes. Any in-flight older
    /// job is cancelled and can no longer publish. Run the job off the UI thread.
    pub fn submit(
        &self,
        bytes: Vec<u8>,
        token: Option<RevisionToken>,
    ) -> Result<Job, SessionError> {
        let mut state = lock(&self.shared.state);
        let generation = state.bump()?;
        let cancel = CancellationToken::new();
        state.inflight = Some(Inflight {
            generation,
            cancel: cancel.clone(),
            token: token.clone(),
        });
        Ok(Job {
            generation,
            cancel,
            bytes,
            token,
            shared: Arc::downgrade(&self.shared),
        })
    }
    /// Report a platform change notification. Never reads files.
    pub fn external_change(
        &self,
        change: ExternalChange,
    ) -> Result<ChangeDisposition, SessionError> {
        let mut state = lock(&self.shared.state);
        let availability = match change {
            ExternalChange::Modified { token } => {
                if token.is_none() {
                    return Ok(ChangeDisposition::ReadRequired);
                }
                if let Some(inflight) = state
                    .inflight
                    .as_ref()
                    .filter(|i| !i.cancel.is_cancelled() && i.token == token)
                {
                    return Ok(ChangeDisposition::Pending(Generation(inflight.generation)));
                }
                let known =
                    state.availability == Availability::Available && token == state.accepted;
                return Ok(if known {
                    ChangeDisposition::Unchanged
                } else {
                    ChangeDisposition::ReadRequired
                });
            }
            ExternalChange::Deleted => Availability::Deleted,
            ExternalChange::Unreadable(reason) => Availability::Unreadable(reason),
        };
        // Duplicate notices: nothing changed since the state was recorded.
        if state.availability == availability && state.inflight.is_none() && state.last > 0 {
            return Ok(ChangeDisposition::Recorded(Generation(state.last)));
        }
        let generation = state.bump()?;
        state.availability = availability;
        state.accepted = None;
        Ok(ChangeDisposition::Recorded(Generation(generation)))
    }
    /// Cancel the in-flight submission without superseding it. Returns whether
    /// a pending job was cancelled.
    pub fn cancel(&self) -> bool {
        let mut state = lock(&self.shared.state);
        match state.inflight.take() {
            Some(inflight) => {
                let live = !inflight.cancel.is_cancelled();
                inflight.cancel.cancel();
                live
            }
            None => false,
        }
    }
    /// Newest published snapshot, which may be stale; see [`status`](Self::status).
    pub fn current(&self) -> Option<Arc<Snapshot>> {
        lock(&self.shared.state).published.clone()
    }
    pub fn status(&self) -> SessionStatus {
        let state = lock(&self.shared.state);
        let generation = |g: u64| (g > 0).then_some(Generation(g));
        SessionStatus {
            latest: generation(state.last),
            published: state.published.clone(),
            confirmed: generation(state.confirmed),
            availability: state.availability,
            pending: state
                .inflight
                .as_ref()
                .filter(|i| !i.cancel.is_cancelled())
                .map(|i| Generation(i.generation)),
            failure: state.failure.clone(),
        }
    }
}

/// Parse work for one generation. Owns its bytes; `Send`, so it can move to
/// any worker. Dropping it unrun clears the pending state for its generation.
#[must_use = "a submitted job publishes nothing until it is run"]
pub struct Job {
    generation: u64,
    cancel: CancellationToken,
    bytes: Vec<u8>,
    token: Option<RevisionToken>,
    shared: Weak<Shared>,
}
impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("generation", &self.generation)
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}
enum Resolve {
    Publish(Arc<Snapshot>),
    Unchanged(Arc<Snapshot>),
    Fail(SessionError),
}
impl Job {
    pub fn generation(&self) -> Generation {
        Generation(self.generation)
    }
    /// Token observed by this job's parse; cancelling it never affects others.
    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancel
    }
    /// Validate, deduplicate, parse and publish if still the newest generation.
    pub fn run(mut self) -> Outcome {
        self.execute()
    }
    fn execute(&mut self) -> Outcome {
        let generation = Generation(self.generation);
        let Some(shared) = self.shared.upgrade() else {
            return Outcome::Closed { generation };
        };
        let (options, document_id) = (shared.options, shared.document_id.clone());
        // Read the published source without holding the lock during comparison.
        let published = match self.gate(&shared) {
            Ok(published) => published,
            Err(outcome) => return outcome,
        };
        // Do not keep the session alive while parsing; dropping it cancels us.
        drop(shared);
        let bytes = std::mem::take(&mut self.bytes);
        let resolve = if bytes.len() > options.limits.model.max_source_bytes {
            Resolve::Fail(SessionError::Parse(ParseError::SourceLimit))
        } else {
            match String::from_utf8(bytes) {
                Err(_) => Resolve::Fail(SessionError::InvalidUtf8),
                Ok(text) => match published.filter(|p| p.source() == text) {
                    Some(snapshot) => Resolve::Unchanged(snapshot),
                    None => match self.parse(text, document_id, options) {
                        Ok(snapshot) => Resolve::Publish(Arc::new(snapshot)),
                        Err(CancellableParseError::Cancelled) => return self.finish(None),
                        Err(CancellableParseError::Parse(e)) => {
                            Resolve::Fail(SessionError::Parse(e))
                        }
                    },
                },
            }
        };
        self.finish(Some(resolve))
    }
    /// Early exit for superseded/cancelled/closed jobs; otherwise the current
    /// published snapshot for deduplication.
    fn gate(&self, shared: &Shared) -> Result<Option<Arc<Snapshot>>, Outcome> {
        let mut state = lock(&shared.state);
        let generation = Generation(self.generation);
        if state.last != self.generation {
            return Err(Outcome::Superseded {
                generation,
                latest: Generation(state.last),
            });
        }
        if self.cancel.is_cancelled() {
            state.clear_inflight(self.generation);
            return Err(Outcome::Cancelled { generation });
        }
        Ok(state.published.clone())
    }
    fn parse(
        &self,
        text: String,
        document_id: DocumentId,
        options: SessionOptions,
    ) -> Result<Snapshot, CancellableParseError> {
        let revision = RevisionId(format!("g{}", self.generation));
        let source: Arc<str> = Arc::from(text);
        let model = match options.model {
            ModelVersion::V1 => SnapshotModel::V1(parse_markdown_cancellable(
                &source,
                document_id,
                revision,
                options.limits,
                &self.cancel,
            )?),
            #[cfg(feature = "model-v2")]
            ModelVersion::V2 => SnapshotModel::V2(crate::parse_markdown_v2_cancellable(
                &source,
                document_id,
                revision,
                options.limits,
                &self.cancel,
            )?),
        };
        Ok(Snapshot {
            generation: Generation(self.generation),
            source,
            model,
            blocks: OnceLock::new(),
        })
    }
    /// Publication point: the only place session state accepts a result.
    /// `None` means the parse observed cancellation.
    fn finish(&mut self, resolve: Option<Resolve>) -> Outcome {
        let generation = Generation(self.generation);
        let Some(shared) = self.shared.upgrade() else {
            return Outcome::Closed { generation };
        };
        let mut state = lock(&shared.state);
        if state.last != self.generation {
            return Outcome::Superseded {
                generation,
                latest: Generation(state.last),
            };
        }
        state.clear_inflight(self.generation);
        let resolve = match resolve {
            Some(resolve) if !self.cancel.is_cancelled() => resolve,
            _ => return Outcome::Cancelled { generation },
        };
        state.availability = Availability::Available;
        state.accepted = self.token.take();
        match resolve {
            Resolve::Fail(error) => {
                state.failure = Some((generation, error.clone()));
                Outcome::Failed { generation, error }
            }
            Resolve::Unchanged(snapshot) => {
                // Only this (newest) generation could have published since the
                // gate, so `snapshot` is still the published value.
                state.failure = None;
                state.confirmed = self.generation;
                Outcome::Unchanged(snapshot)
            }
            Resolve::Publish(snapshot) => {
                state.failure = None;
                state.confirmed = self.generation;
                state.published = Some(Arc::clone(&snapshot));
                Outcome::Published(snapshot)
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            lock(&shared.state).clear_inflight(self.generation);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_exhaustion_leaves_state_unchanged() {
        let session =
            DocumentSession::new(DocumentId("doc".into()), SessionOptions::default()).unwrap();
        let job = session.submit(b"kept".to_vec(), None).unwrap();
        lock(&session.shared.state).last = u64::MAX;
        // The in-flight job now holds a stale generation; reseed it as newest.
        let mut job = job;
        job.generation = u64::MAX;
        lock(&session.shared.state)
            .inflight
            .as_mut()
            .unwrap()
            .generation = u64::MAX;
        assert_eq!(
            session.submit(b"x".to_vec(), None).err(),
            Some(SessionError::GenerationExhausted)
        );
        assert_eq!(
            session.external_change(ExternalChange::Deleted).err(),
            Some(SessionError::GenerationExhausted)
        );
        let status = session.status();
        assert_eq!(status.latest, Some(Generation(u64::MAX)));
        assert_eq!(status.pending, Some(Generation(u64::MAX)));
        assert_eq!(status.availability, Availability::Available);
        assert!(!job.cancellation_token().is_cancelled());
        match job.run() {
            Outcome::Published(snapshot) => {
                assert_eq!(snapshot.generation(), Generation(u64::MAX));
                assert_eq!(snapshot.source(), "kept");
            }
            other => panic!("{other:?}"),
        }
    }
}
