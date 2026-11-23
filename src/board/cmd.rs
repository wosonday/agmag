//! Types shared by the command dispatcher (binary crate) and the full-screen app.

use crate::dashboard::Section;
use crate::project::ProjectKey;

/// Colour of a one-line footer note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Ok,
    Warn,
    Err,
}

/// What one command line produced.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Note(NoteKind, String),
    /// A text block to show (`show`, `cd` list, `cleanup` candidates).
    Shown(String),
    /// The next input line answers this confirmation (`y` confirms, anything else cancels).
    NeedsConfirm(String),
    OpenBoard(ProjectKey),
    Quit,
    None,
}

/// `dispatch` result: the outcome plus the sections currently chosen with `view`.
#[derive(Debug, Clone, PartialEq)]
pub struct Dispatched {
    pub outcome: Outcome,
    pub view: Vec<Section>,
}
