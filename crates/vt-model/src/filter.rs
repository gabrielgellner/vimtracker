//! A track's filter: its type, and how that type responds.

use serde::{Deserialize, Serialize};

/// A track's filter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterType {
    #[default]
    LowPass,
    HighPass,
    BandPass,
    Notch,
    /// A saturating four-pole ladder, which self-oscillates and costs more.
    Ladder,
    /// A saturating diode ladder, the kind in Roland's TB-303, which costs more too.
    Diode,
    /// The Korg 35 low-pass in Korg's MS-20, which screams as it self-oscillates and costs more.
    Ms20LowPass,
    /// The Korg 35's high-pass, from the same circuit.
    Ms20HighPass,
    /// Vital's `Dirty` low-pass, whose resonance loop saturates, which costs more too.
    Dirty,
    /// The OB-X's two-pole state-variable filter, after OB-Xd's, whose damping falls as it gets
    /// quieter, so it sings by itself near the top of `Res`, and costs more too.
    ObX,
}

impl FilterType {
    /// Every type, in the order a list shows them.
    pub const ALL: [Self; 10] = [
        Self::LowPass,
        Self::HighPass,
        Self::BandPass,
        Self::Notch,
        Self::Ladder,
        Self::Diode,
        Self::Ms20LowPass,
        Self::Ms20HighPass,
        Self::Dirty,
        Self::ObX,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::LowPass => "LP",
            Self::HighPass => "HP",
            Self::BandPass => "BP",
            Self::Notch => "Notch",
            Self::Ladder => "Ladder",
            Self::Diode => "Diode",
            Self::Ms20LowPass => "MS20 LP",
            Self::Ms20HighPass => "MS20 HP",
            Self::Dirty => "Dirty",
            Self::ObX => "OB-X",
        }
    }

    /// What it does, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::LowPass => "Low-pass, clean, 12 or 24 dB",
            Self::HighPass => "High-pass, clean, 12 or 24 dB",
            Self::BandPass => "Band-pass, clean",
            Self::Notch => "Notch, clean",
            Self::Ladder => "Saturating ladder, low-pass and more, self-oscillates",
            Self::Diode => "Diode ladder low-pass, 303 style, self-oscillates",
            Self::Ms20LowPass => "Korg 35 low-pass, MS-20 style, 12 dB, screams",
            Self::Ms20HighPass => "Korg 35 high-pass, MS-20 style, 6 dB, screams",
            Self::Dirty => "Vital's low-pass and more, saturating resonance",
            Self::ObX => "OB-X state-variable, 12 dB, multimode, sings",
        }
    }

    /// Whether it always saturates, at twice the rate, rather than only when driven.
    pub fn saturates(self) -> bool {
        matches!(
            self,
            Self::Ladder
                | Self::Diode
                | Self::Ms20LowPass
                | Self::Ms20HighPass
                | Self::Dirty
                | Self::ObX
        )
    }

    /// The modes it offers, the one it starts in first. The MS-20's offer none.
    pub fn modes(self) -> &'static [FilterMode] {
        use FilterMode::{
            BandPass, BandPass12, Full, HighPass6, HighPass12, HighPass12LowPass6, HighPass18,
            HighPass18LowPass6, HighPass24, LowCut1, LowCut2, LowCut3, LowPass6, LowPass12,
            LowPass18, LowPass24, LowPass24Fat, Notch, Notch12, Notch12LowPass6, Phaser,
            PhaserLowPass6, Slope12, Slope24,
        };
        match self {
            Self::LowPass | Self::HighPass | Self::BandPass | Self::Notch => &[Slope12, Slope24],
            // The Oberheim Xpander's fifteen, and Vital's notch.
            Self::Ladder => &[
                LowPass24,
                LowPass24Fat,
                LowPass18,
                LowPass12,
                LowPass6,
                BandPass,
                BandPass12,
                HighPass6,
                HighPass12,
                HighPass18,
                HighPass24,
                Notch,
                Notch12,
                Phaser,
                HighPass12LowPass6,
                HighPass18LowPass6,
                Notch12LowPass6,
                PhaserLowPass6,
            ],
            Self::Dirty => &[
                LowPass24, LowPass12, BandPass, HighPass12, HighPass24, Notch,
            ],
            Self::Diode => &[Full, LowCut3, LowCut2, LowCut1],
            // OB-Xd's two-pole morphs from low-pass through band-pass or notch to high-pass.
            Self::ObX => &[LowPass12, BandPass12, HighPass12, Notch12],
            Self::Ms20LowPass | Self::Ms20HighPass => &[],
        }
    }

    /// Whether `Punch` moves it: the clean types, whose state-variable filter it's made for.
    pub fn has_punch(self) -> bool {
        !self.saturates()
    }

    /// Whether `Safe` moves it: the types that ring by themselves at the top of `Res`. The clean
    /// types never do.
    pub fn has_safe(self) -> bool {
        self.saturates()
    }

    /// Whether it has more than one mode, so `Mode` means something on it.
    pub fn has_modes(self) -> bool {
        !self.modes().is_empty()
    }

    /// The mode it plays for a track set to this one: the same, if it offers it, or its first. A
    /// type with no modes plays as it always does, whatever this is.
    pub fn plays(self, mode: FilterMode) -> FilterMode {
        let modes = self.modes();
        if modes.contains(&mode) {
            mode
        } else {
            modes.first().copied().unwrap_or(mode)
        }
    }
}

crate::listed::listed!(FilterType: meaning);

/// How a track's filter type responds: how steep the clean types are, which of their responses the
/// ladder, `Dirty` and `OB-X` give out, or where the diode's low cut sits. Each type offers some of these.
/// A track keeps its mode when its type changes, and a type that doesn't offer it plays its own
/// first, so going to another type and back finds the mode again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterMode {
    /// The clean types as they are, and with a second filter after them, for twice the slope.
    #[default]
    Slope12,
    Slope24,
    /// The ladder's and `Dirty`'s responses, mixed from the stages they already have.
    LowPass24,
    LowPass12,
    BandPass,
    HighPass12,
    HighPass24,
    Notch,
    /// Where the diode's input high-pass sits: far under, as `Acid` has it, or 3, 2 or 1 octaves
    /// under `Cut`, so less bass reaches the ladder.
    Full,
    LowCut3,
    LowCut2,
    LowCut1,
    /// The rest of the Oberheim Xpander's mixes of the ladder's stages, which only the ladder
    /// offers, but for `BP12` and `N12`, which `OB-X` has too. They come after the others, so a
    /// step's value for those stays as it was.
    LowPass18,
    LowPass6,
    BandPass12,
    HighPass6,
    HighPass18,
    Notch12,
    /// Three stages' phase shift against what goes in: a dip above `Cut`.
    Phaser,
    /// A high-pass, notch or phaser and a 6 dB low-pass at once.
    HighPass12LowPass6,
    HighPass18LowPass6,
    Notch12LowPass6,
    PhaserLowPass6,
    /// The ladder's low-pass with what its resonance takes from the bass partly given back.
    LowPass24Fat,
}

impl FilterMode {
    /// Every mode, in the order a step's value counts them.
    pub const ALL: [Self; 24] = [
        Self::Slope12,
        Self::Slope24,
        Self::LowPass24,
        Self::LowPass12,
        Self::BandPass,
        Self::HighPass12,
        Self::HighPass24,
        Self::Notch,
        Self::Full,
        Self::LowCut3,
        Self::LowCut2,
        Self::LowCut1,
        Self::LowPass18,
        Self::LowPass6,
        Self::BandPass12,
        Self::HighPass6,
        Self::HighPass18,
        Self::Notch12,
        Self::Phaser,
        Self::HighPass12LowPass6,
        Self::HighPass18LowPass6,
        Self::Notch12LowPass6,
        Self::PhaserLowPass6,
        Self::LowPass24Fat,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        FILTER_MODE_NAMES[usize::from(self.index())]
    }

    /// What it does, for a list to show.
    pub(crate) fn meaning(self) -> &'static str {
        match self {
            Self::Slope12 => "12 dB an octave, one filter",
            Self::Slope24 => "24 dB an octave, a second filter after the first",
            Self::LowPass24 => "Low-pass, 24 dB",
            Self::LowPass24Fat => "Low-pass, 24 dB, keeps more bass as Res rises",
            Self::LowPass18 => "Low-pass, 18 dB",
            Self::LowPass12 => "Low-pass, 12 dB",
            Self::LowPass6 => "Low-pass, 6 dB",
            Self::BandPass => "Band-pass, 12 dB either side",
            Self::BandPass12 => "Band-pass, 6 dB either side, wider",
            Self::HighPass6 => "High-pass, 6 dB",
            Self::HighPass12 => "High-pass, 12 dB",
            Self::HighPass18 => "High-pass, 18 dB",
            Self::HighPass24 => "High-pass, 24 dB",
            Self::Notch => "Notch, 24 dB",
            Self::Notch12 => "Notch, 12 dB, narrower",
            Self::Phaser => "Phaser: a dip above Cut, the bass thinned",
            Self::HighPass12LowPass6 => "High-pass 12 dB and low-pass 6 dB, a lopsided band",
            Self::HighPass18LowPass6 => "High-pass 18 dB and low-pass 6 dB, a lopsided band",
            Self::Notch12LowPass6 => "Notch 12 dB and low-pass 6 dB",
            Self::PhaserLowPass6 => "Phaser and low-pass 6 dB",
            Self::Full => "All the bass goes in, as in Acid",
            Self::LowCut3 => "Low cut 3 octaves under Cut",
            Self::LowCut2 => "Low cut 2 octaves under Cut",
            Self::LowCut1 => "Low cut 1 octave under Cut",
        }
    }

    /// Its place in the list, which is a step's value for it.
    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0) as u8
    }

    /// The mode at a place in the list, if there's one there.
    pub fn at(index: u8) -> Option<Self> {
        Self::ALL.get(usize::from(index)).copied()
    }
}

crate::listed::listed!(FilterMode: meaning);

/// Each mode's name, in the list's order, for a step's value to show.
pub const FILTER_MODE_NAMES: [&str; 24] = [
    "12 dB", "24 dB", "LP24", "LP12", "BP24", "HP12", "HP24", "N24", "Full", "-3 oct", "-2 oct",
    "-1 oct", "LP18", "LP6", "BP12", "HP6", "HP18", "N12", "PH18", "HP12+LP6", "HP18+LP6",
    "N12+LP6", "PH18+LP6", "Fat24",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_filter_type_plays_a_mode_it_offers_and_keeps_its_own() {
        for kind in FilterType::ALL {
            for mode in FilterMode::ALL {
                let played = kind.plays(mode);
                match kind.modes() {
                    [] => assert_eq!(played, mode, "{kind:?} has none to fall back on"),
                    modes if modes.contains(&mode) => assert_eq!(played, mode),
                    modes => assert_eq!(played, modes[0], "{kind:?} {mode:?}"),
                }
            }
        }
        for mode in FilterMode::ALL {
            assert_eq!(FilterMode::at(mode.index()), Some(mode));
        }
        let mut names = FILTER_MODE_NAMES.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), FilterMode::ALL.len(), "each name says which");
    }
}
