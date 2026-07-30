# Invariant / fuzz tests

Note on location: actual tests live per-crate, in `programs/leash-program/tests/`
(Cargo's native convention — `cargo test -p leash-program`), not in this top-level
directory. Shared test helpers live in `programs/leash-program/tests/common/mod.rs`.

One test per line, each proving a violation attempt fails:

- [x] spend exactly at cap succeeds; one unit over fails — `week3_spend_enforcement.rs`
      (the old caveat — that the over-cap case was observably identical to Token-2022's
      own insufficient-balance rejection — no longer applies. The same file now also
      spends past a *delegating* parent's remaining budget while it still holds the full
      balance, which Token-2022 is happy with, so the `CapExceeded` there is provably the
      hook's. Both cases assert their specific error code. See docs/ROADMAP.md 0.5.)
- [x] spend before expiry succeeds; one second after fails —
      `week4_ancestor_chain_and_fuzz.rs::spend_respects_expiry_boundary`
- [x] spend to an allowlisted destination succeeds; any other destination fails —
      `week3_spend_enforcement.rs`
- [x] revoke a capability, then spend from it — fails — `week3_spend_enforcement.rs`
- [x] revoke a *parent*, then spend from a *child* — fails — `week3_spend_enforcement.rs`
      (one ancestor level); `week4_ancestor_chain_and_fuzz.rs` extends this to all three
      ancestor levels a depth-3 capability can have, each isolated in its own fresh
      chain so revoking the wrong level can't accidentally make the test pass:
      `depth3_spend_fails_when_immediate_parent_revoked`,
      `depth3_spend_fails_when_grandparent_revoked`,
      `depth3_spend_fails_when_root_revoked`, plus the positive case
      `depth3_spend_succeeds_when_nothing_revoked`.
- [x] attenuate a child with cap > parent's remaining budget — fails —
      `week4_ancestor_chain_and_fuzz.rs::attenuate_rejects_child_cap_exceeding_parent_remaining`
      (also checks the boundary: exactly at the parent's remaining budget succeeds)
- [x] attenuate past MAX_DEPTH — fails —
      `week4_ancestor_chain_and_fuzz.rs::attenuate_rejects_depth_past_max_depth`

All items checked. What this suite is *not*: property-based/randomized fuzzing in the
AFL/honggfuzz sense — every scenario above is a specific, hand-constructed case, not a
generated one. "Fuzz" in BUILD_PLAN.md's original phrasing meant this checklist; genuine
randomized fuzzing (e.g. via `trident` or `proptest` over instruction sequences) remains
a real gap if deeper assurance is ever needed before real funds touch this program.

Mandatory before touching the CLI or SDK (docs/BUILD_PLAN.md §8). A bug here is, per the
build plan, a total-loss bug — treat this directory as gating, not optional.
