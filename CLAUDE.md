# Leash

Spending authority for AI agents as a bearer object, enforced by the token itself via a
Token-2022 Transfer Hook — not by a server, dashboard, or the agent's own code. Full spec:
[docs/BUILD_PLAN.md](docs/BUILD_PLAN.md). Original pitch: [leash.txt](leash.txt).

**Read docs/BUILD_PLAN.md before writing any instruction logic.** It defines the six
non-negotiable properties (§2), the data model (§3), the exact instruction set (§4), and
four unvalidated design decisions — D1-D4 (§5) — that must be spiked and confirmed in
Week 1 before anything else is built on top of them. Do not treat D1-D4 as settled; they
are the first thing to test, not an assumption to code against.

## Current state

Scaffolding only. Two Anchor programs exist as typed stubs (`Accounts` structs + `todo!()`
handlers) matching BUILD_PLAN.md §3/§4 exactly — no business logic yet:

- `programs/leash-program` — Capability state; `issue` / `attenuate` / `revoke` / `redeem`.
- `programs/leash-hook` — Token-2022 TransferHook `execute`; this is where cap/expiry/
  allowlist/revoked enforcement will live, on every spend.

Workspace compiles (`cargo check --workspace`) as of scaffold time. Nothing is deployed.

## Explicit non-goals for this phase

Sharded budgets, ZK/state compression, merkle-proof allowlists, velocity limits,
confidential transfers, secp256r1/passkey signing, framework adapters, `leash-verify`
middleware, x402 integration, a dashboard, a hosted service, mainnet deployment. See
BUILD_PLAN.md §9 and §12 — these are real, later, and deliberately not started.

## Milestones (BUILD_PLAN.md §7)

Week 1 spike (D1-D4) → Week 2 issue/redeem → Week 3 attenuate + hook spend-path →
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
