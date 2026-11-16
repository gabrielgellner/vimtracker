//! The engine's seeded generator, so renders are repeatable. It's kept with the project's other
//! generators in `vt_model::random` (decision 361).

/// PCG32 with the multiplier the engine has always used, so saved seeds play as they did. Each
/// track has its own, so adding a track doesn't change what other tracks do.
pub type Pcg32 = vt_model::random::EnginePcg32;
