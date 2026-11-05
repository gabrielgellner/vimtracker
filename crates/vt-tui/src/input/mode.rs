//! The editing modes.

/// What keys currently mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Normal,
    /// After an operator, waiting for what it covers.
    OperatorPending,
    /// Selecting fields.
    VisualField,
    /// Selecting whole steps.
    VisualStep,
    Insert,
    Command,
    Help,
}

impl Mode {
    /// The name shown on the status line.
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal | Self::OperatorPending => "NORMAL",
            Self::VisualField => "VISUAL",
            Self::VisualStep => "V-STEP",
            Self::Insert => "INSERT",
            Self::Command => "COMMAND",
            Self::Help => "HELP",
        }
    }

    /// Whether this selects.
    pub fn is_visual(self) -> bool {
        matches!(self, Self::VisualField | Self::VisualStep)
    }
}
