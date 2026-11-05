//! The editing modes and views.

/// What fills the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Chain,
    Phrase,
    Instrument,
}

impl View {
    /// Every view, in the order the picker starts with.
    pub const ALL: [Self; 3] = [Self::Phrase, Self::Chain, Self::Instrument];

    /// The view's name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Chain => "chain",
            Self::Phrase => "phrase",
            Self::Instrument => "instrument",
        }
    }
}

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
    Picker,
    /// Waiting for an answer about restoring an autosave.
    Restore,
    /// Normal mode's keys in the chain view. Only the binding table and the label use this.
    Chain,
    /// Normal mode's keys in the instrument view. Only the binding table and the label use this.
    Instrument,
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
            Self::Picker => "PICK",
            Self::Restore => "RESTORE",
            Self::Chain => "CHAIN",
            Self::Instrument => "INST",
        }
    }

    /// Whether this selects.
    pub fn is_visual(self) -> bool {
        matches!(self, Self::VisualField | Self::VisualStep)
    }
}
