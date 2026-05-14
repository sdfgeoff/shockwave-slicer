use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlicePhase {
    LoadModel,
    Voxelize,
    PropagateField,
    ExtractLayers,
    ClipLayers,
    GeneratePaths,
    WriteGcode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SliceProgress {
    pub phase: SlicePhase,
    pub phase_progress: f32,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}
