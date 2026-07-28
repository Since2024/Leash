# Leash

Spending authority for AI agents as a bearer object, enforced by the token itself via a
Token-2022 Transfer Hook — not by a server, dashboard, or the agent's own code. Full spec:
[docs/BUILD_PLAN.md](docs/BUILD_PLAN.md). Original pitch: [leash.txt](leash.txt).

**Read docs/BUILD_PLAN.md before writing any instruction logic.** It defines the six
non-negotiable properties (§2), the data model (§3), and the exact instruction set (§4).
D1-D4 (§5) were spiked in Week 1 and all hold — see "Week 1 spike results" in §5 for what
that spike actually found (it wasn't clean: a `fallback`-dispatch requirement, a PDA
rent-funding bug, real dependency version conflicts, and a Solana CLI upgrade were all
necessary, not just the four original questions). Read those findings before touching
`leash-hook` — they change how `initialize_extra_account_meta_list` and the fallback path
must be written, not just whether the approach works at all.

## Current state

Week 1 spike complete and passing (`programs/leash-hook/tests/spike_d1_d4.rs`, run via
`cargo test -p leash-hook --test spike_d1_d4`). `leash-program`'s four instructions
(`issue`/`attenuate`/`revoke`/`redeem`) are still typed stubs with `todo!()` bodies —
Week 2's job. `leash-hook` has real, working (if minimal) logic: it registers an extra
account and, on a real Token-2022 transfer, reads source/destination and logs — proven
against a built `.so`, not just compiled. No cap/expiry/allowlist/revoked enforcement yet;
that's Week 3, once D4's fixed-placeholder extra account is replaced with a real,
per-source Capability PDA lookup.

Solana CLI upgraded to Agave 4.1.1 / platform-tools v1.54 during the spike (the
originally-installed 1.18.26 couldn't build the current dependency tree at all). Workspace
compiles (`cargo check --workspace`) and `leash-hook` builds to a real `.so`
(`cargo-build-sbf --manifest-path programs/leash-hook/Cargo.toml`). Nothing is deployed to
devnet or mainnet.

## Explicit non-goals for this phase

Sharded budgets, ZK/state compression, merkle-proof allowlists, velocity limits,
confidential transfers, secp256r1/passkey signing, framework adapters, `leash-verify`
middleware, x402 integration, a dashboard, a hosted service, mainnet deployment. See
BUILD_PLAN.md §9 and §12 — these are real, later, and deliberately not started.

## Milestones (BUILD_PLAN.md §7)

Week 1 spike (D1-D4) ✅ → Week 2 issue/redeem → Week 3 attenuate + hook spend-path →
Week 4 ancestor-chain + fuzz suite → Week 5 SDK/CLI → Week 6 demo + grant submission.

## Repo layout

```
leash/
  programs/leash-program/   Capability state, issue/attenuate/revoke/redeem
  programs/leash-hook/       Token-2022 TransferHook enforcement
  tests/integration/          Anchor/LiteSVM happy-path tests (Week 2+)
  tests/invariants/           Fuzz/property tests for BUILD_PLAN.md §2's six properties (Week 4, gating)
  sdk/ts/                     TypeScript SDK (Week 5, not started)
  cli/                        CLI (Week 5, not started)
  docs/BUILD_PLAN.md
```

## Stay on devnet

Real-money temptation is a named risk (BUILD_PLAN.md §10). Do not deploy to mainnet with
real USDC until the invariant/fuzz suite in §8 is complete.
