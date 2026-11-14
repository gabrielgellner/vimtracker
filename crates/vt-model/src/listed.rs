//! The things a list offers to choose from, such as the machines, the effects, the mod types and
//! the filter types: each in its place, with its name and what it does. Pickers, `K` and the
//! Roto's knobs build their lists from these (`docs/designs/device-interface.md` §4.4).

/// A kind of thing with a fixed list of its values, each named, and no two by the same name. A
/// type whose values share a name, as `Order`'s snakes do, or as generator settings on different
/// fills do, isn't one.
pub trait Listed: Copy + PartialEq + 'static {
    /// Every value, in the order lists show them and `-` and `=` go through them.
    const ALL: &'static [Self];

    /// The name on screen.
    fn name(self) -> &'static str;

    /// What it does, for a list to show, or nothing.
    fn meaning(self) -> &'static str {
        ""
    }

    /// The category a list shows it under, or none.
    fn category(self) -> &'static str {
        ""
    }

    /// What a song or a preset stores it by, where that's a key of its own rather than its
    /// serde name, or nothing.
    fn key(self) -> &'static str {
        ""
    }
}

/// The names of every one of `X`, in order: the items of its picker, or the steps of a Roto knob
/// that chooses one.
pub fn choices_of<X: Listed>() -> impl ExactSizeIterator<Item = &'static str> + Clone {
    X::ALL.iter().map(|value| value.name())
}

/// Implements [`Listed`] for a type from its own `ALL` and `name`, and from those of its
/// `meaning`, `category` and `key` that are named after the colon, as `listed!(Machine: meaning,
/// category)`. A type keeps its own methods, so calling them needs no import.
macro_rules! listed {
    ($kind:ty $(: $($method:ident),+)?) => {
        impl $crate::listed::Listed for $kind {
            const ALL: &'static [Self] = &<$kind>::ALL;

            fn name(self) -> &'static str {
                <$kind>::name(self)
            }

            $($(
                fn $method(self) -> &'static str {
                    <$kind>::$method(self)
                }
            )+)?
        }
    };
}

pub(crate) use listed;
