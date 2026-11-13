//! The editing modes and views.

/// What fills the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Song,
    Chain,
    Phrase,
    Track,
    Mixer,
    /// The project page's Audio sub-page: this machine's output, its device, rate and buffer.
    Audio,
    /// The project page's Song sub-page: the song's own settings, and saving, rendering and
    /// opening.
    Project,
    /// The cursor's track's sample, as a big waveform.
    Sample,
}

impl View {
    /// Every view, in the order the picker starts with.
    pub const ALL: [Self; 8] = [
        Self::Phrase,
        Self::Chain,
        Self::Song,
        Self::Track,
        Self::Mixer,
        Self::Project,
        Self::Audio,
        Self::Sample,
    ];

    /// The view's name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Song => "song",
            Self::Chain => "chain",
            Self::Phrase => "phrase",
            Self::Track => "track",
            Self::Mixer => "mixer",
            Self::Audio => "audio",
            Self::Project => "project",
            Self::Sample => "sample",
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
    /// Waiting for what replaces the field under the cursor.
    Replace,
    Command,
    Help,
    /// The shape panel is open.
    Shape,
    /// A macro's mapping view is open, its settings and their ranges (decision 285).
    Mapping,
    /// Searching the track page for a setting to jump to.
    Jump,
    /// Reading a guide.
    Guide,
    Picker,
    /// Waiting for an answer about restoring an autosave.
    Restore,
    /// The keyboard plays the track's sound, from the track page, without writing anything.
    Keys,
    /// Normal mode's keys in the chain view. Only the binding table and the label use this.
    Chain,
    /// Normal mode's keys on the track page. Only the binding table and the label use this.
    Track,
    /// Normal mode's keys on the mixer. Only the binding table and the label use this.
    Mixer,
    /// Normal mode's keys in the song view. Only the binding table and the label use this.
    Song,
    /// Normal mode's keys on the audio page. Only the binding table and the label use this.
    Audio,
    /// Normal mode's keys on the project page. Only the binding table and the label use this.
    Project,
    /// Normal mode's keys on the sample page. Only the binding table and the label use this.
    Sample,
    /// The sample page's keys while it selects, with its text objects. Only the binding table and
    /// the label use this.
    SampleVisual,
}

impl Mode {
    /// The name shown on the status line.
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal | Self::OperatorPending => "NORMAL",
            Self::VisualField => "VISUAL",
            Self::VisualStep => "V-STEP",
            Self::Insert => "INSERT",
            Self::Replace => "REPLACE",
            Self::Command => "COMMAND",
            Self::Help => "HELP",
            Self::Shape => "SHAPE",
            Self::Mapping => "MAP",
            Self::Jump => "JUMP",
            Self::Guide => "GUIDE",
            Self::Picker => "PICK",
            Self::Restore => "RESTORE",
            Self::Keys => "KEYS",
            Self::Chain => "CHAIN",
            Self::Track => "TRACK",
            Self::Mixer => "MIX",
            Self::Song => "SONG",
            Self::Audio => "AUDIO",
            Self::Project => "PROJECT",
            Self::Sample => "SAMPLE",
            Self::SampleVisual => "VISUAL",
        }
    }

    /// Whether this selects.
    pub fn is_visual(self) -> bool {
        matches!(self, Self::VisualField | Self::VisualStep)
    }
}
