//! A state for each side of a stereo sound, for what works on each side alone with the same
//! settings, worked out once while both sides are the same (decision 147).

/// What can say whether it's exactly the same as another, bit for bit, so working out one in place
/// of both gives exactly what both would. A float's `==` won't do: it has `0.0` and `-0.0` the same
/// and a NaN never so.
pub(crate) trait SameBits {
    fn same_bits(&self, other: &Self) -> bool;
}

impl SameBits for f32 {
    #[inline]
    fn same_bits(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl<T: SameBits, const N: usize> SameBits for [T; N] {
    #[inline]
    fn same_bits(&self, other: &Self) -> bool {
        self.iter().zip(other).all(|(a, b)| a.same_bits(b))
    }
}

/// Whether a frame's two sides are the same, bit for bit.
#[inline]
pub(crate) fn same_sides(frame: [f32; 2]) -> bool {
    frame[0].same_bits(&frame[1])
}

/// Each side's state. While a frame is the same on both sides and so are the states, the left is
/// worked out alone and given to both, and the right's state is left as it was, as it would only
/// have become the left's. The moment the sides differ, the right's is copied from the left's and
/// both are worked out again, so it comes out exactly as working out both every frame does. A sound
/// that has differed goes back to being worked out once when the states are the same again: when
/// they come to the same bits, which a filter's may never quite do, or when what holds them starts
/// them again from nothing, as a pedal does once it has rung out.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sides<T> {
    states: [T; 2],
    /// Whether the right's state is the left's, so it isn't kept up to date.
    together: bool,
}

impl<T: Default> Default for Sides<T> {
    fn default() -> Self {
        Self {
            states: [T::default(), T::default()],
            together: true,
        }
    }
}

impl<T: Copy + SameBits> Sides<T> {
    /// One frame through `step`, which works one side out from its state and its input. `same`
    /// says the frame's two sides are the same, bit for bit, as the slot before found or made
    /// them. False is always safe: it works out both.
    #[inline]
    pub(crate) fn process(
        &mut self,
        input: [f32; 2],
        same: bool,
        mut step: impl FnMut(&mut T, f32) -> f32,
    ) -> [f32; 2] {
        if !same && self.together {
            self.states[1] = self.states[0];
            self.together = false;
        } else if same && !self.together {
            self.together = self.states[0].same_bits(&self.states[1]);
        }
        let [left, right] = &mut self.states;
        if self.together {
            let out = step(left, input[0]);
            return [out; 2];
        }
        [step(left, input[0]), step(right, input[1])]
    }

    /// The states being kept up to date: the left's alone while the sides are together.
    pub(crate) fn current(&self) -> &[T] {
        &self.states[..if self.together { 1 } else { 2 }]
    }

    /// Both states, for settings each keeps, which are the same on both sides.
    pub(crate) fn both_mut(&mut self) -> &mut [T; 2] {
        &mut self.states
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-pole low-pass, enough of a state to tell the sides apart.
    #[derive(Debug, Clone, Copy, Default)]
    struct Pole(f32);

    impl SameBits for Pole {
        fn same_bits(&self, other: &Self) -> bool {
            self.0.same_bits(&other.0)
        }
    }

    fn step(pole: &mut Pole, input: f32) -> f32 {
        pole.0 += (input - pole.0) * 0.1;
        pole.0
    }

    #[test]
    fn the_same_on_both_sides_is_worked_out_once() {
        let mut sides = Sides::<Pole>::default();
        let mut calls = 0;
        for at in 0..10 {
            let input = at as f32;
            sides.process([input; 2], true, |pole, x| {
                calls += 1;
                step(pole, x)
            });
        }
        assert_eq!(calls, 10);
        assert_eq!(sides.current().len(), 1);
    }

    #[test]
    fn it_comes_out_as_both_worked_out_would() {
        let mut sides = Sides::<Pole>::default();
        let mut apart = [Pole::default(); 2];
        let inputs = (0..400).map(|at| match at {
            100..150 | 300 => [1.0, -1.0],
            _ => [0.5; 2],
        });
        for input in inputs {
            let out = sides.process(input, same_sides(input), step);
            let expected = [step(&mut apart[0], input[0]), step(&mut apart[1], input[1])];
            assert_eq!(out.map(f32::to_bits), expected.map(f32::to_bits));
        }
    }

    #[test]
    fn the_sides_come_back_together_once_their_states_are_the_same() {
        // A pole that follows at once is the same on both sides a frame after they are.
        let follow = |pole: &mut Pole, input: f32| {
            pole.0 = input;
            pole.0
        };
        let mut sides = Sides::<Pole>::default();
        sides.process([1.0, -1.0], false, follow);
        assert_eq!(sides.current().len(), 2);
        sides.process([0.5; 2], true, follow);
        assert_eq!(sides.current().len(), 2, "the states differed going in");
        sides.process([0.5; 2], true, follow);
        assert_eq!(sides.current().len(), 1);
    }

    #[test]
    fn a_zero_of_the_other_sign_is_a_difference() {
        assert!(!0.0_f32.same_bits(&-0.0));
        assert!(!f32::NAN.same_bits(&-f32::NAN));
        assert!(1.5_f32.same_bits(&1.5));
        assert!(!same_sides([0.0, -0.0]));
    }
}
