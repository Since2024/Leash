# Leash

Spending authority for AI agents as a bearer object, enforced by the token itself via a
Token-2022 Transfer Hook — not by a server, dashboard, or the agent's own code. Full spec:
[docs/BUILD_PLAN.md](docs/BUILD_PLAN.md). Original pitch: [leash.txt](leash.txt).

**Read docs/BUILD_PLAN.md before touching either program.** It defines the six
non-negotiable properties (§2), the data model (§3), and the exact instruction set (§4).
D1-D4 (§5) were spiked in Week 1 and all hold, but read every "Week N results" entry in
§5 before assuming the current shape is obvious — every week so far found real,
non-cosmetic bugs (a `fallback`-dispatch requirement, a PDA rent-funding bug, a
`Capability.parent` layout constraint, an Anchor CPI signer gotcha, and — Week 4 — a
chained-account-resolution design that broke on root capabilities) that changed the
design, not just confirmed it.

## Current state

All 6 weeks of the MVP plan are complete. `leash-hook` has **real, full
enforcement** — cap, expiry, allowlist, revoked, and the entire ancestor chain to
`MAX_DEPTH`:

- `programs/leash-program/tests/common/mod.rs` — shared test helpers (setup, issue,
  attenuate, revoke, redeem, spend) used by all five test files below.
- `week2_issue_redeem.rs` — Week 2: a full deposit -> issue -> redeem round trip.
- `week3_spend_enforcement.rs` — Week 3: `attenuate` and a real Token-2022 transfer
  checked against cap/expiry/allowlist/revoked, including one ancestor level.
- `week4_ancestor_chain_and_fuzz.rs` — Week 4: the ancestor check extended to the full
  `MAX_DEPTH` chain (a depth-3 capability's spend is independently verified to fail when
  *any* of its three ancestors — immediate parent, grandparent, or root — is revoked, not
  just the first), plus `attenuate`'s cap/depth boundary rejections and the expiry
  boundary. Closes every item on `tests/invariants/README.md`'s checklist.
- `conservation_invariant.rs` — post-MVP: proves `spent + committed_to_children <= cap`
  is actually enforced on the spend path. A parent could spend budget it had already
  delegated to a child, so a tree could spend more than the deposit backing it
  (docs/ROADMAP.md 0.2, now fixed). Verified to fail without the fix.
- `redeem_authorization.rs` — post-MVP: proves `redeem` can't be used to walk around the
  hook. Burning fires no transfer hook, and `redeem` consulted no `Capability`, so a
  delegated agent could cash out its unspent budget to any address (docs/ROADMAP.md 0.1,
  now fixed). Redemption is now gated on who funded the vault: merchants freely, a root
  only up to `cap - spent - committed_to_children` (with `cap` written back down), a
  delegated capability never. Verified to fail without the fix.
- `reclaim_and_descendant_revoke.rs` — post-MVP: proves a principal can cut off one agent
  and get the money back. `revoke` was `has_one = owner`, so a parent's only lever was
  revoking itself, which cascades to every descendant (docs/ROADMAP.md 0.8); and nothing
  ever decremented `committed_to_children`, so a dead child's budget was stranded forever
  (0.7). Now `revoke_descendant` lets any ancestor revoke a descendant (authority proved
  from the target's own `ancestors` array), and `reclaim` lets the **immediate parent**
  release `cap - spent` once the child is revoked or expired. `reclaim` moves accounting
  only — it cannot burn the child's units, since that token account's authority is the
  child, not the program (see `reclaim.rs`, and 0.10 for the supply consequence). The
  liveness guard was verified to be load-bearing.
- `multi_capability.rs` — post-MVP: proves one owner can hold **many** capabilities, and
  that they stay independent (budget, spending, revocation). Capabilities used to be
  keyed `[CAPABILITY_SEED, owner]`, so a second `issue`/`attenuate` for the same owner
  collided inside Anchor's `init` (docs/ROADMAP.md 0.3, now fixed). Each capability now
  owns a token account at `[TOKEN_ACCOUNT_SEED, owner, nonce]` and is keyed off *that*
  address; the hook re-derives it from base account 0 and additionally checks
  `source == capability.token_account` (0.4). Verified to fail without the fix.

**Two assertion helpers, and the difference matters.** `expect_err`
(`tests/common/mod.rs`) checks only that a call failed — it prints the error without
inspecting it. `expect_err_code` asserts a specific on-chain error code, and is what
essentially every rejection test now uses (docs/ROADMAP.md 0.5, closed). A rejection test
therefore proves the transaction failed *for the reason it is named after*, and the
assertions were verified to discriminate — deliberately asserting a neighbouring code
makes them fail.

Exactly two `expect_err` calls survive, both in `multi_capability.rs`, both deliberate and
justified at the call site: one rejection is genuinely bound by token balance rather than
by the cap (the isolated version of that check sits directly above it), and the other
fails during client-side extra-account resolution, where no on-chain error code exists to
assert. Prefer `expect_err_code` for anything new; reach for `expect_err` only when you
can write down why no code is assertable.

**Cap enforcement is now tested in isolation from balance.** `issue` mints exactly `cap`,
so for a fresh capability the balance and the remaining budget are the same number and
Token-2022's own check fires first. `attenuate` breaks that tie — it mints the child's
units fresh, so a parent that delegated still holds the full balance while its spendable
budget has shrunk. That is the lever used to test the hook's arithmetic on its own.
Prefer `expect_err_code` for anything new. Week 1's placeholder spike test is retired —
superseded by Week 3.

`revoke` is a real one-line flip, joined by `revoke_descendant` (an ancestor revoking
someone below it) and `reclaim` (releasing a dead child's reservation). Nothing is stubbed
in either program anymore.
"Fuzz suite" here still means a checklist of specific hand-built cases, not
generated/randomized fuzzing — see `tests/invariants/README.md`'s closing note if
that distinction matters for what you're about to rely on.

Solana CLI is Agave 4.1.1 / platform-tools v1.54 (upgraded during Week 1). Workspace
compiles (`cargo check --workspace --tests`), both programs build to real `.so` files via
`cargo-build-sbf --manifest-path programs/<name>/Cargo.toml`.

**Build the `.so` with `cargo-build-sbf`, not `anchor build`.** An `anchor build` binary
fails on a local `solana-test-validator` at ~44 compute units with `Access violation in
unknown section`, before any program logic runs — `anchor build` swaps the sbpf toolchain
out from under the build. Use `anchor build` only to regenerate the IDL (`target/idl/`,
`target/types/` → `sdk/ts/src/idl/`; hand-patching the IDL instead is what made an earlier
attempt at ROADMAP 0.3 untrustworthy), then rebuild with `cargo-build-sbf` and restart the
validator against those binaries.

**The recovery step has a trap of its own.** Running `cargo-build-sbf` right after
`anchor build` often *no-ops* — cargo sees the crate as up-to-date and never relinks, so
`anchor build`'s incompatible binary silently stays in `target/deploy/`. It looks like it
worked (`Finished ... in 0.26s`). Force it: `rm -f target/deploy/*.so && touch
programs/*/src/lib.rs` before rebuilding, and check the `.so` md5 actually changed. This
cost two debugging cycles; the give-away is a build that finishes suspiciously fast.

Both programs were deployed to **devnet** (`leash-program`:
`Gbx7nEL2rxWUTj7LnqRQtBDU7yi8oF3miYmjKGncsDXk`, `leash-hook`:
`9WPQUY6zVRwVZ3eUsDF1aNESWAyZwL8GwKpzd2C66xtS`) — nothing on mainnet. **That deployment
is now stale:** ROADMAP 0.3 changed the instruction ABI (`issue`/`attenuate` take a
`nonce`, account lists changed), so it needs a redeploy, not an upgrade over live state.

`sdk/ts/` (`@leash/sdk`) and `cli/` (`@leash/cli`) are real, not stubs — every
instruction has a typed wrapper, and the full loop (`init` -> `mint` -> `spend` ->
fetch/decode -> `revoke` -> `spend` fails on-chain) was proven end-to-end against a real
`solana-test-validator`, not just type-checked.

Post-0.3, `mint`/`attenuate` take an optional nonce and **return the one they used** —
keep it, it re-derives the capability's addresses. `spend`/`revoke`/`watch`/`attenuate`
require an explicit `--capability` or `--nonce` and refuse to guess, since defaulting
would silently act on the wrong capability for anyone holding several. `leash list`
(`findCapabilitiesByOwner`) recovers the set by scanning program accounts; it cannot
recover a *random* nonce (the nonce is hashed into an address, so `--recover-nonce`
brute-forces small sequential values only) and says so instead of printing a blank. See BUILD_PLAN.md's "Week 5 results" for
the four real bugs this surfaced (Anchor's JS coder lowercasing account names, missing
feePayer/recentBlockhash on raw transactions, `anchor build`'s stricter CHECK-comment
lint, and a stale-ledger false-crash).

## Explicit non-goals for this phase

Sharded budgets, ZK/state compression, merkle-proof allowlists, velocity limits,
confidential transfers, secp256r1/passkey signing, framework adapters, `leash-verify`
middleware, x402 integration, a dashboard, a hosted service, mainnet deployment. See
BUILD_PLAN.md §9 and §12 — these are real, later, and deliberately not started.

## Milestones (BUILD_PLAN.md §7)

Week 1 spike (D1-D4) ✅ → Week 2 issue/redeem ✅ → Week 3 attenuate + hook spend-path ✅ →
Week 4 ancestor-chain + fuzz suite ✅ → Week 5 SDK/CLI ✅ → Week 6 devnet deploy + demo ✅.
The grant application has been submitted; nothing remains on this list. Ongoing work is
tracked in docs/ROADMAP.md, not here.

## Repo layout

```
leash/
  programs/leash-program/
    src/                      Capability state, issue/attenuate/revoke/revoke_descendant/
                                reclaim/redeem/record_spend
    tests/
      common/mod.rs           Shared test helpers (setup, issue, attenuate, revoke, redeem, spend)
      week2_issue_redeem.rs
      week3_spend_enforcement.rs
      week4_ancestor_chain_and_fuzz.rs
      conservation_invariant.rs
      redeem_authorization.rs
      multi_capability.rs
      reclaim_and_descendant_revoke.rs
  programs/leash-hook/
    src/                      Token-2022 TransferHook enforcement
  tests/invariants/           Tracking checklist for BUILD_PLAN.md §8 — fully checked as
                               of Week 4; actual tests live in programs/leash-program/tests/
  sdk/ts/                     TypeScript SDK (Week 5, complete)
  cli/                        CLI (Week 5, complete)
  docs/BUILD_PLAN.md          Phase 1 MVP build plan (complete)
  docs/ROADMAP.md             Path to a usable, mainnet-trustworthy product (not a company roadmap)
```

Note: tests live per-crate (`programs/<name>/tests/`, Cargo's native convention), not in
a top-level `tests/integration/` — that directory (from the original scaffold) turned out
unused and has been removed.

## Stay on devnet

Real-money temptation is a named risk (BUILD_PLAN.md §10). Do not deploy to mainnet with
real USDC until the invariant/fuzz suite in §8 is complete.
