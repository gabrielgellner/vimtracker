//! The Roto's identities, pinned: every layout's `kind`, the `PluginHash` a stored setup of it is
//! found by, and each stored setup's version. The Roto keeps a setup under its hash and vimtracker
//! writes it again when its version changes, so a change to any of these leaves the user's Roto
//! with setups vimtracker no longer knows, or writes them all again. Change a row only on purpose,
//! never as a refactor's side effect (#731, the device design's P0b).

use std::collections::BTreeMap;

use vt_roto::PluginHash;
use vt_roto::store::NAMESPACE;
use vt_tui::roto::devices::stored_setups;
use vt_tui::roto::layouts::{ALL, GENERATORS};

/// Each layout's kind, its stored hash in hex and its stored setup's version, if it's stored.
#[rustfmt::skip]
const PINS: &[(&str, &str, Option<u64>)] = &[
    ("accumulator", "5a307e1f78587813", None),
    ("adsr", "424b4e61293e3f12", Some(0xcd78443b1b248093)),
    ("ahd", "5a07600558463116", Some(0x6ac099d2900e839a)),
    ("arpeggio", "4d0246581a565a57", Some(0xc763c12937d33337)),
    ("drum", "2e4a7f664b110856", Some(0xb85f83c73b7ab338)),
    ("filter", "5429415763571841", Some(0x756f3b86935c7488)),
    ("fx/beads", "111d3b013624760a", Some(0xe9725f77e5007ff9)),
    ("fx/chorus", "156654564f3a0a35", None),
    ("fx/clouds", "235230280f0a5776", Some(0xd8a7d432fd02edd7)),
    ("fx/comb", "492b050300391e34", None),
    ("fx/comp-clean", "1e1e3d16357f3c29", Some(0x7bd858c254d44dd9)),
    ("fx/comp-console", "77550d2d0e14696f", Some(0x42e803274e20a528)),
    ("fx/comp-glue", "016b22070018166e", Some(0xd2438a9bf72ecaef)),
    ("fx/delay", "0f235234370c1841", Some(0x7b8223a3d178f438)),
    ("fx/distort-crush", "1758007d187b3a71", Some(0xbed4c99ed7e1fcf5)),
    ("fx/distort-down", "3f425b13386a286d", Some(0x2789f528c438ef5c)),
    ("fx/distort-pedal", "7548782a581b3635", None),
    ("fx/distort", "3f6328760c782d3a", None),
    ("fx/echo", "4e1f1006297d0053", None),
    ("fx/empty", "28270e06131c0471", None),
    ("fx/eq3", "222e47606d156d43", None),
    ("fx/exciter", "7d1d643337167d6f", None),
    ("fx/flanger", "5d4f676e543f1719", None),
    ("fx/freqshift", "481e302678387e5a", None),
    ("fx/grains", "7d7560767e604e4a", Some(0xeab1fa1cd3232e97)),
    ("fx/limit", "5362072e381d6736", None),
    ("fx/lpg", "42226527676c5110", None),
    ("fx/phaser", "16625053261b513f", None),
    ("fx/pitch", "7c605f4834722055", None),
    ("fx/plate", "363001024b75043c", None),
    ("fx/reso", "7f69103e1b240634", None),
    ("fx/reverb", "0f0b212a6c1b0b13", None),
    ("fx/rotary", "082450217e3e793f", Some(0x864918d7bca4b138)),
    ("fx/space", "13002f034f6e7d2b", None),
    ("fx/spring", "507139444f12353f", Some(0x61abb73b42822a54)),
    ("fx/stutter", "170c26233c201a7d", Some(0xc0575d242744c372)),
    ("fx/tape", "5c4f0c557c04483d", None),
    ("fx/vowel", "280e61103f2e0c5a", None),
    ("fx/warps", "67762d75243d0410", None),
    ("gen/acid-loops", "736551432f04680b", None),
    ("gen/acid", "2b0e4e0232580f19", None),
    ("gen/alternate", "0319704b62592168", None),
    ("gen/bits", "7e2f764d43507f2a", None),
    ("gen/divisions", "07626a237b09131b", None),
    ("gen/follow", "21677c3742233c4a", None),
    ("gen/grids", "6b4c30382e646f7e", None),
    ("gen/lane", "40597901277f7f62", None),
    ("gen/list", "5f2b20107046781f", None),
    ("gen/multiply", "105e5c6a164d2950", None),
    ("gen/pink", "5f3a7b6f3d444634", None),
    ("gen/ramp", "4a72732d27247064", None),
    ("gen/random", "055f2e3b56406368", None),
    ("gen/rework", "0c10211d6a254d44", None),
    ("gen/ring", "06653b684c493b4a", None),
    ("gen/scale", "4f4d7639203e6e0a", None),
    ("gen/tonnetz", "7c431a223f5a6b19", None),
    ("gen/variation", "332d027b783b5b1b", None),
    ("gen/walk", "66120641077f1e35", None),
    ("gen/wave", "3a101e4e5a43276c", None),
    ("lfo", "176065263d512027", Some(0xfa005b05a8218b3b)),
    ("lpg", "48225825727e3928", Some(0xa2b2e5b7bd766315)),
    ("machine/4op", "0819466672703213", None),
    ("machine/4op/op1", "2a7a61030f2f4c08", None),
    ("machine/4op/op2", "294a0d6b0a76627a", None),
    ("machine/4op/op3", "0a4c145b426f6a3c", None),
    ("machine/4op/op4", "216f6f682675241d", None),
    ("machine/6-op", "3a76130249540e45", None),
    ("machine/acid", "022d3d1a7f4c247c", None),
    ("machine/additive", "513f5c2829720604", None),
    ("machine/bell", "68386f323d6a0a68", None),
    ("machine/blown", "187f4e0e2a5e5f73", None),
    ("machine/bowed", "6b176b691f555578", None),
    ("machine/chaosfm", "78644b0901736d59", None),
    ("machine/chip", "190a4f085c370d77", None),
    ("machine/chord", "74104750127b687c", None),
    ("machine/clocked", "3609320317537475", None),
    ("machine/csaw", "617626004f61515f", None),
    ("machine/drum", "1e1e056e1a34570c", None),
    ("machine/dust", "6b3c761f140a7459", None),
    ("machine/fbfm", "486d2d5750364c31", None),
    ("machine/fluted", "7e0d3551277c557c", None),
    ("machine/fm", "457a57052c370447", None),
    ("machine/fof", "4323520d2d4e5800", None),
    ("machine/fold", "434b481141316f0a", None),
    ("machine/formant", "564d51021c691279", None),
    ("machine/graincloud", "4656002739215a2c", None),
    ("machine/hat", "77677f4b51075e74", None),
    ("machine/kick", "0d770a5652594e4f", None),
    ("machine/modal", "106a634f1f5d153b", None),
    ("machine/modem", "5c7028181b37581c", None),
    ("machine/morph", "564d54320b272a70", None),
    ("machine/noise", "2f2d420f1e564264", None),
    ("machine/paraphonic", "626e1b3a2f041602", None),
    ("machine/pd", "106750254e1c5a15", None),
    ("machine/ring", "220a7d684478712b", None),
    ("machine/sampler", "430f011531147806", None),
    ("machine/sawcomb", "35475b2c1c131f40", None),
    ("machine/sawsquare", "4907364156360c72", None),
    ("machine/snare", "0d3d3b394003582b", None),
    ("machine/speech", "6247793145530d05", None),
    ("machine/string", "1d0313130c1b6d1e", None),
    ("machine/supersaw", "1c5f2a210929427d", None),
    ("machine/swarm", "4c0203340f515c1f", None),
    ("machine/sync", "20502159430a3e26", None),
    ("machine/terrain", "3b1f0c055b194e28", None),
    ("machine/toy", "543265674e4c4772", None),
    ("machine/triple", "007b177a212e792b", None),
    ("machine/twinpeaks", "4c1f12321a0e3316", None),
    ("machine/va-vcf", "353e26262b435c58", None),
    ("machine/va", "1f70542538034e33", None),
    ("machine/vosim", "5c4a0b0434495b54", None),
    ("machine/waveline", "467e0324703d570a", None),
    ("machine/wavemap", "68136b1442576526", None),
    ("machine/waves", "66696d3317653657", None),
    ("machine/zfilter", "641f6f3a30730542", None),
    ("macros", "22481c0b344f6c54", None),
    ("output", "206258332816370c", None),
    ("output/group", "310c05455e575b00", None),
    ("output/master", "14077a623b5d3e47", Some(0xc6747ad58c0eaa2f)),
    ("output/return", "490e4451674a0c1a", None),
    ("play", "77663f6163017b2e", None),
    ("steps", "3d0652311d450b51", None),
    ("track", "2d11422176703174", Some(0x6b981ef3a71ebc0c)),
];

fn hex(hash: PluginHash) -> String {
    hash.0.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn every_layouts_kind_hash_and_stored_version_is_as_pinned() {
    let setups = stored_setups();
    let found: BTreeMap<&str, (String, Option<u64>)> = ALL
        .iter()
        .chain(GENERATORS.iter())
        .map(|layout| {
            let key = format!("{NAMESPACE}{}", layout.kind);
            let version = setups
                .iter()
                .find(|setup| setup.key == key)
                .map(|setup| setup.version());
            (layout.kind, (hex(PluginHash::of(&key)), version))
        })
        .collect();
    let pinned: BTreeMap<&str, (String, Option<u64>)> = PINS
        .iter()
        .map(|(kind, hash, version)| (*kind, ((*hash).to_owned(), *version)))
        .collect();
    assert_eq!(pinned.len(), PINS.len(), "each kind pinned once");
    assert_eq!(found, pinned);
    // Every stored setup is a pinned layout's.
    let stored = PINS.iter().filter(|(_, _, version)| version.is_some());
    assert_eq!(stored.count(), setups.len());
}
