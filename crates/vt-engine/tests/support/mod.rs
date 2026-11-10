//! Golden renders, shared by the tests that keep them.

use std::path::PathBuf;

/// FNV-1a over the samples' exact bits, so a golden catches any change at all.
fn hash(samples: &[f32]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for sample in samples {
        for byte in sample.to_bits().to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// Compare with a saved hash. `VT_BLESS=1` updates it after an intended change to the sound, and
/// `VT_DUMP=folder` writes the samples there, to compare two builds by how far apart they are.
///
/// These hold on one platform: the filter and reverb use library math that can differ elsewhere.
pub fn golden(name: &str, samples: &[f32]) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.hash"));
    let found = hash(samples);
    if let Some(dump) = std::env::var_os("VT_DUMP") {
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        std::fs::write(format!("{}/{name}.raw", dump.to_string_lossy()), bytes).unwrap();
    }
    if std::env::var_os("VT_BLESS").is_some() {
        std::fs::write(&path, format!("{found}\n")).unwrap();
        return;
    }
    let saved = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{} is missing: run with VT_BLESS=1", path.display()));
    assert_eq!(
        saved.trim(),
        found,
        "{name} changed: run with VT_BLESS=1 if that was intended"
    );
}
