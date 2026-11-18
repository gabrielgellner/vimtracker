//! Analog colour (#262): the saturator frame of [`crate::dsp::blocks::saturator`] in a slot, its
//! settings eased so moving them doesn't step.

use crate::dsp::blocks::saturator::{Colouring, Curve, Saturator, Tuning};
use crate::dsp::blocks::sides::{SameBits, Sides, same_bits_by_fields};
use crate::dsp::blocks::smoother::Smoothed;

/// How far `Drive` turns up what reaches the curve at `FF`, in dB.
pub(crate) const MOST_DRIVE_DB: f32 = 24.0;

/// What a slot's colour is set to, each from `00` to `FF`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ColourSettings {
    pub curve: Curve,
    pub drive: u8,
    pub even: u8,
    pub hard: u8,
    /// Centred: none at `80`.
    pub emphasis: u8,
    pub iron: u8,
}

/// A slot's colour: each side's frame and the settings, eased.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Colour {
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

impl Default for Colour {
    fn default() -> Self {
        // The tables are worked out here, off the audio thread, rather than the first time a
        // slot plays one.
        Curve::prepare();
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

same_bits_by_fields!(Colour {
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

impl Colour {
    /// Take what's set.
    pub(crate) fn follow(&mut self, settings: ColourSettings) {
        let share = |value: u8| f32::from(value) / 255.0;
        self.curve = settings.curve;
        self.drive.target = share(settings.drive) * MOST_DRIVE_DB;
        self.even.target = share(settings.even);
        self.hard.target = share(settings.hard);
        self.emphasis.target = vt_model::insert::from_middle(f32::from(settings.emphasis));
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
