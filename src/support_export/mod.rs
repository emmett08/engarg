//! Compile canonical obligations into one portable executable EAL program.
pub(crate) mod compiler;
pub use compiler::{compile, compile_direct, operation_tool_name};
// Optional authoring-route adapters; resolved execution does not use a catalogue.
pub use crate::support_graph::{AcquisitionRouteProof, project_execution_graph};
