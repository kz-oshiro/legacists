//! Offline analysis without GUI, application state, or persistence dependencies.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

mod analysis;
mod decode;
pub mod diagnostics;
pub mod input;
mod markup;
mod model;
mod syntax;
pub use analysis::{analyze, verify_inputs, InputChange};
mod export;
mod query;
pub use export::{
    plan_export, reconstruct_sources, render_export, ExportOptions, ExportPart, ExportPlan,
};
pub use input::{FileInput, InputError, InputProvider};
pub use model::*;
pub use query::{query, Query, QueryResult};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    pub(crate) fn check(&self) -> Result<(), CoreError> {
        if self.is_cancelled() {
            Err(CoreError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreError {
    Cancelled,
    InputBoundary,
    InvalidId,
    InvalidCursor,
    InvalidRange,
    DecodeFailed,
    EmptySelection,
    InvalidLimit,
    InputChanged,
    Internal,
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CoreError {}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Progress {
    pub stage: &'static str,
    pub completed: usize,
    pub total: Option<usize>,
}
