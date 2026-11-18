//! The `Colour` effect (#262): the saturator frame of [`crate::dsp::blocks::saturator`] in a slot,
//! its settings eased so moving them doesn't step.

use vt_model::insert::{
    Colour as Settings, ColourType, MOST_COLOUR_EMPHASIS_DB, colour_drive_db, colour_emphasis_db,
};

use crate::dsp::blocks::saturator::{Colouring, Curve, Saturator, Tuning};
use crate::dsp::blocks::sides::{SameBits, Sides, same_bits_by_fields};
use crate::dsp::blocks::smoother::Smoothed;
use crate::dsp::effects::memory::SongPlace;
use crate::dsp::effects::slot::{Mono, SlotEffect};

/// A slot's colour: each side's frame and the settings, eased.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Saturation {
    sides: Sides<Saturator>,
    curve: Curve,
    drive: Smoothed,
    even: Smoothed,
    hard: Smoothed,
    emphasis: Smoothed,
    iron: Smoothed,
    /// The filters, worked out for the emphasis, `Iron` and the rate they were last at.
    tuning: Tuning,
    tuned: [f32; 2],
    rate: u32,
}

impl Default for Saturation {
    fn default() -> Self {
        Self {
            sides: Sides::default(),
            curve: Curve::Warm,
            drive: Smoothed::default(),
            even: Smoothed::default(),
            hard: Smoothed::default(),
            emphasis: Smoothed::default(),
            iron: Smoothed::default(),
            tuning: Tuning::default(),
            tuned: [f32::NAN; 2],
            rate: 0,
        }
    }
}

impl SameBits for Curve {
    #[inline]
    fn same_bits(&self, other: &Self) -> bool {
        self == other
    }
}

same_bits_by_fields!(Saturation {
    sides,
    curve,
    drive,
    even,
    hard,
    emphasis,
    iron,
    tuning,
    tuned,
    rate,
});

impl Saturation {
    /// Take what's set.
    pub(crate) fn follow(&mut self, settings: Settings) {
        let share = |value: u8| f32::from(value) / 255.0;
        self.curve = match settings.kind {
            ColourType::Warm => Curve::Warm,
            ColourType::Tube => Curve::Tube,
            ColourType::Diode => Curve::Diode,
            ColourType::Transistor => Curve::Transistor,
        };
        self.drive.target = colour_drive_db(f32::from(settings.drive));
        self.even.target = share(settings.even);
        self.hard.target = share(settings.hard);
        self.emphasis.target =
            colour_emphasis_db(f32::from(settings.emph)) / MOST_COLOUR_EMPHASIS_DB;
        self.iron.target = share(settings.iron);
    }

    /// Jump to what's set.
    pub(crate) fn settle(&mut self) {
        for setting in [
            &mut self.drive,
            &mut self.even,
            &mut self.hard,
            &mut self.emphasis,
            &mut self.iron,
        ] {
            setting.settle();
        }
    }

    /// One frame through the colour, all wet. `same` says the frame's sides are the same, bit for
    /// bit.
    #[inline]
    #[allow(
        clippy::float_cmp,
        reason = "the filters are worked out again only when what they're for moves at all"
    )]
    pub(crate) fn process(
        &mut self,
        input: [f32; 2],
        same: bool,
        smoothing: f32,
        rate: u32,
    ) -> [f32; 2] {
        let colouring = Colouring {
            curve: self.curve,
            drive: 10.0_f32.powf(self.drive.next(smoothing) / 20.0),
            even: self.even.next(smoothing),
            hard: self.hard.next(smoothing),
        };
        let tuned = [self.emphasis.next(smoothing), self.iron.next(smoothing)];
        if tuned != self.tuned || rate != self.rate {
            (self.tuned, self.rate) = (tuned, rate);
            self.tuning = Tuning::new(tuned[0], tuned[1], rate);
        }
        let tuning = &self.tuning;
        self.sides.process(input, same, |side, input| {
            side.process(input, colouring, tuning)
        })
    }
}

impl SlotEffect for Saturation {
    type Settings = Settings;

    /// Each side is worked alone, the same way, with nothing between them.
    const MONO: Mono = Mono::OneSide;

    fn new(_rate: u32) -> Self {
        Self::default()
    }

    fn follow(&mut self, settings: Settings, _song: SongPlace, _frames: u64) {
        self.follow(settings);
    }

    fn settle(&mut self) {
        self.settle();
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    /// None: it's worked every frame, so the slot never waits on its tail.
    fn quiet_frames(&self, _rate: u32) -> u32 {
        0
    }
}
