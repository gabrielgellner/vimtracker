//! Letting a filter's state fall to an exact zero once it's far below anything heard (#260).
//!
//! Left to itself after a sound, a filter's state falls into the denormal floats and, rounded
//! there, can trade the smallest of them between its states for good, so what comes after it
//! hears something for ever: a delay rings on, and a mix that rests once nothing moves never does.
//! The processor's flush-to-zero doesn't help: it drops the tiny steps a state moves by, so the
//! state stops a little above the denormals, at about 1e-37, and stays there. So each filter lets
//! its state go itself, with nothing coming in, once all of it is below [`TINY`].

/// Below this a filter's state is let go: about -400 dB, so small that it can't change a bit of
/// anything above about 1e-13 (-260 dB) it's added to, and far over where the floats run out.
pub(crate) const TINY: f32 = 1e-20;

/// What can say whether all it keeps is below [`TINY`].
pub(crate) trait Tiny {
    fn tiny(&self) -> bool;
}

impl Tiny for f32 {
    #[inline]
    fn tiny(&self) -> bool {
        self.abs() < TINY
    }
}

impl Tiny for f64 {
    #[inline]
    fn tiny(&self) -> bool {
        self.abs() < f64::from(TINY)
    }
}

impl<T: Tiny, const N: usize> Tiny for [T; N] {
    #[inline]
    fn tiny(&self) -> bool {
        self.iter().all(Tiny::tiny)
    }
}

/// [`Tiny`] for a filter's state, field by field. Every field must be named, as the struct is
/// taken apart without `..`, so a field added later can't be left out by mistake.
macro_rules! tiny_by_fields {
    ($kind:ident { $($field:ident),* $(,)? }) => {
        impl $crate::dsp::blocks::flush::Tiny for $kind {
            #[inline]
            fn tiny(&self) -> bool {
                let $kind { $($field),* } = self;
                $($crate::dsp::blocks::flush::Tiny::tiny($field))&&*
            }
        }
    };
}
pub(crate) use tiny_by_fields;

/// Start a filter's state again from nothing once all of it is below [`TINY`], for a frame with
/// nothing coming in. It goes all at once rather than value by value, as letting one of a loop's
/// values go while the others ring on would hold the rest back: a resonant filter's band let go
/// each frame leaves its low-pass to creep down over seconds.
#[inline]
pub(crate) fn flush<T: Tiny + Default>(state: &mut T) {
    if state.tiny() {
        *state = T::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_goes_only_once_all_of_it_is_tiny() {
        let mut state = [1e-21, -1e-40, f32::MIN_POSITIVE, -0.0];
        flush(&mut state);
        assert_eq!(state.map(f32::to_bits), [0.0_f32.to_bits(); 4]);
        let mut ringing = [1e-21, 1e-19];
        flush(&mut ringing);
        assert_eq!(ringing, [1e-21, 1e-19]);
        let mut broken = [1e-21, f32::NAN];
        flush(&mut broken);
        assert!(broken[1].is_nan());
    }
}
