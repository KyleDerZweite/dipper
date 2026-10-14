use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetIdentity {
    pub window_id: String,
    pub pid: u32,
    pub application: String,
}

#[derive(Debug, Clone)]
pub struct TargetPermit {
    pub identity: TargetIdentity,
    valid: Arc<AtomicBool>,
}

impl TargetPermit {
    pub fn new(identity: TargetIdentity) -> Self {
        Self { identity, valid: Arc::new(AtomicBool::new(true)) }
    }
    pub fn revoke(&self) { self.valid.store(false, Ordering::SeqCst); }
    pub fn is_valid(&self) -> bool { self.valid.load(Ordering::SeqCst) }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DispatchStage { Unchanged, ClipboardPublished, PasteDispatched }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus { Suppressed, Unverified, Confirmed, Failed }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertionOutcome {
    pub stage: DispatchStage,
    pub status: DeliveryStatus,
    pub detail: String,
}

#[async_trait]
pub trait Insertion: Send + Sync {
    async fn arm(&self) -> anyhow::Result<TargetPermit>;
    async fn dispatch(&self, permit: &TargetPermit, text: &str) -> InsertionOutcome;
    async fn copy_explicit(&self, text: &str) -> anyhow::Result<()>;
}
