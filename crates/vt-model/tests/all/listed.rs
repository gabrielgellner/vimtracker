//! The lists that pickers, `K` and the Roto's knobs offer: each `Listed` type's values, in the
//! order of its own `ALL`, each named once.

use std::collections::HashSet;
use std::fmt::Debug;

use vt_model::accumulator::{Adds, AtLimit, Moves};
use vt_model::arpeggio::{ArpMode, ArpRate};
use vt_model::bank::Warp;
use vt_model::envelope::Curve;
use vt_model::insert::{DistortType, EffectKind, Insert};
use vt_model::lfo::{LfoMode, Shape};
use vt_model::listed::{Listed, choices_of};
use vt_model::modulation::{ModKind, Polarity};
use vt_model::scale::ScaleKind;
use vt_model::shape::{
    Bar, CellStart, Channel, Division, DrumPart, FillKind, Moves as TonnetzMoves, Role,
    TonnetzVoice, World,
};
use vt_model::song::{FilterMode, FilterType, Machine, Role as TrackRole, SynthVoice};
use vt_model::speed::Speed;
use vt_model::step::GateKind;
use vt_model::steps::StepsMode;
use vt_model::tracking::Source;

/// Checks one type's list against its own `ALL`, and that a list names each value once.
fn check<X: Listed + Debug>(own: &[X]) {
    assert_eq!(X::ALL, own, "the list is the type's own ALL");
    let names: Vec<&str> = choices_of::<X>().collect();
    assert_eq!(names.len(), own.len());
    for (name, value) in names.iter().zip(own) {
        assert_eq!(*name, value.name());
        assert!(!name.is_empty(), "{value:?} has a name");
    }
    let unique: HashSet<&str> = names.iter().copied().collect();
    assert_eq!(unique.len(), names.len(), "{names:?} names each once");
    let keys: Vec<&str> = own.iter().map(|value| value.key()).collect();
    if keys.iter().any(|key| !key.is_empty()) {
        let unique: HashSet<&str> = keys.iter().copied().collect();
        assert!(
            !unique.contains(""),
            "{keys:?}: each has a key, or none does"
        );
        assert_eq!(unique.len(), keys.len(), "{keys:?} are each one's own");
    }
}

#[test]
fn every_list_is_its_types_own_and_names_each_once() {
    check(&Adds::ALL);
    check(&ArpMode::ALL);
    check(&ArpRate::ALL);
    check(&AtLimit::ALL);
    check(&Bar::ALL);
    check(&CellStart::ALL);
    check(&Channel::ALL);
    check(&Curve::ALL);
    check(&DistortType::ALL);
    check(&Division::ALL);
    check(&DrumPart::ALL);
    check(&EffectKind::ALL);
    check(&FillKind::ALL);
    check(&FilterMode::ALL);
    check(&FilterType::ALL);
    check(&GateKind::ALL);
    check(&Insert::ALL);
    check(&LfoMode::ALL);
    check(&Machine::ALL);
    check(&ModKind::ALL);
    check(&Moves::ALL);
    check(&Polarity::ALL);
    check(&Role::ALL);
    check(&ScaleKind::ALL);
    check(&Shape::ALL);
    check(&Source::ALL);
    check(&Speed::ALL);
    check(&StepsMode::ALL);
    check(&SynthVoice::ALL);
    check(&TonnetzMoves::ALL);
    check(&TonnetzVoice::ALL);
    check(&TrackRole::ALL);
    check(&Warp::ALL);
    check(&World::ALL);
}

/// A list's meanings and categories are the type's own, and a type without them has none.
#[test]
fn a_list_says_what_its_type_says() {
    for machine in Machine::ALL {
        assert_eq!(Listed::meaning(machine), machine.meaning());
        assert_eq!(Listed::category(machine), machine.category());
    }
    for effect in EffectKind::ALL {
        assert_eq!(Listed::key(effect), effect.key());
        assert_eq!(Listed::meaning(effect), "");
    }
    for filter in FilterType::ALL {
        assert_eq!(Listed::meaning(filter), filter.meaning());
        assert_eq!(Listed::category(filter), "");
    }
    for shape in Shape::ALL {
        assert_eq!(Listed::meaning(shape), "");
        assert_eq!(Listed::key(shape), "");
    }
}
