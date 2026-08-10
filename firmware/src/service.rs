//! Core-0-only service state and admission glue.

use alumina_safety::SafetyState;
use alumina_storage::{
    CacheLimits, ContentId, Error, MutationContext, PublishToken, PublishedObject,
    UploadCoordinator, UploadId, UploadPlan, UploadProgress, VerifiedChunk,
};

/// Initial bounded protocol/cache policy; free SD capacity remains a runtime limit.
pub const CACHE_LIMITS: CacheLimits = CacheLimits {
    maximum_object_bytes: 64 * 1_024 * 1_024,
    maximum_chunk_bytes: 1_024,
    maximum_chunks: 65_536,
};

/// Storage transaction state intentionally owned by the service-core task future.
pub struct StorageServiceState {
    uploads: UploadCoordinator,
    safety_state: SafetyState,
    realtime_job_active: bool,
}

impl StorageServiceState {
    /// Starts in boot state, where every mutation is forbidden.
    pub const fn new() -> Self {
        Self {
            uploads: UploadCoordinator::new(),
            safety_state: SafetyState::Boot,
            realtime_job_active: false,
        }
    }

    /// Updates the last core-1-authoritative safety state observation.
    pub fn observe_safety_state(&mut self, state: SafetyState) {
        self.safety_state = state;
    }

    /// Updates whether deterministic execution/prefetch currently owns the cache.
    pub fn observe_realtime_job_active(&mut self, active: bool) {
        self.realtime_job_active = active;
    }

    /// Starts or resumes one decoded `StorageBeginUpload` declaration.
    pub fn begin_upload(&mut self, plan: UploadPlan) -> Result<UploadProgress, Error> {
        self.uploads
            .begin(plan, CACHE_LIMITS, self.mutation_context())
    }

    /// Hashes and validates the next received chunk without mutating durable state.
    pub fn verify_chunk(
        &self,
        upload_id: UploadId,
        index: u32,
        claimed_content: ContentId,
        bytes: &[u8],
    ) -> Result<VerifiedChunk, Error> {
        self.uploads
            .verify_chunk(upload_id, index, claimed_content, bytes)
    }

    /// Records a verified chunk only after the async SD backend has made it durable.
    pub fn record_chunk(&mut self, chunk: VerifiedChunk) -> Result<UploadProgress, Error> {
        self.uploads.record_chunk(chunk, self.mutation_context())
    }

    /// Enters publish-pending after the backend streams both canonical hashes.
    pub fn finalize_upload(
        &mut self,
        upload_id: UploadId,
        observed_object: ContentId,
        observed_manifest: ContentId,
    ) -> Result<PublishToken, Error> {
        self.uploads.finalize(
            upload_id,
            observed_object,
            observed_manifest,
            self.mutation_context(),
        )
    }

    /// Clears the journal only after backend atomic publication succeeds.
    pub fn record_published(&mut self, token: PublishToken) -> Result<PublishedObject, Error> {
        self.uploads
            .record_published(token, self.mutation_context())
    }

    /// Abandons an upload; durable unreferenced chunks remain scrub/GC candidates.
    pub fn abort_upload(&mut self, upload_id: UploadId) -> Result<Option<UploadPlan>, Error> {
        self.uploads.abort(upload_id, self.mutation_context())
    }

    /// Returns bounded progress without exposing the coordinator or filesystem.
    pub fn upload_progress(&self) -> Option<UploadProgress> {
        self.uploads.checkpoint().map(|checkpoint| UploadProgress {
            upload_id: checkpoint.plan.upload_id,
            phase: checkpoint.phase,
            next_chunk: checkpoint.next_chunk,
            accepted_bytes: checkpoint.accepted_bytes,
            total_bytes: checkpoint.plan.object.byte_len,
        })
    }

    fn mutation_context(&self) -> MutationContext {
        let safe_for_mutation = matches!(
            self.safety_state,
            SafetyState::Safe | SafetyState::Configured
        );
        MutationContext {
            armed_or_energized: !safe_for_mutation,
            realtime_job_active: self.realtime_job_active,
        }
    }
}

impl Default for StorageServiceState {
    fn default() -> Self {
        Self::new()
    }
}
