# Leash

**Spending authority for AI agents as a bearer object — enforced by the token itself, not by a server, a dashboard, or the agent's own code.**

Give an agent a $20 budget, a 4-hour expiry, and a merchant allowlist. It cannot physically exceed them: the constraint is checked inside the token transfer itself, by a Solana Token-2022 Transfer Hook, before the transfer is allowed to complete. Revoke access and the very next spend attempt fails — instantly, everywhere the capability is held, with no dependency on any service staying up.

## The problem

Agent authority today is enforced by whatever software happens to be watching — a policy server, a spend-tracking dashboard, a try/catch block in the agent's own runtime. If that software is compromised, buggy, or simply not in the loop for a given call, the limit doesn't exist.

This is not hypothetical:

- A 2026 Cloud Security Alliance survey found **65% of enterprises** running AI agents had at least one agent-related security incident in the prior twelve months, and **35%** of those reported direct financial loss.
- Average monthly AI token spend among Ramp's customers rose **13x** from January 2025 to mid-2026.
- One documented case: four agents, an eleven-day loop, a **$47,000 invoice** — with a spend dashboard, Slack alerts at 50/80/95%, and a provider-level cap all in place. Observability did not stop the spend.

The common failure mode: every existing safeguard enforces the limit in the same place it's trying to protect against — the agent's own execution environment. Leash moves enforcement into the one place an agent (or whoever compromised it) cannot talk its way around: the asset itself.

## How it works

1. A principal deposits a real asset (e.g. USDC) and receives a **Capability** — a Solana account recording a budget (`cap`), an expiry, an allowlist of destinations, and a revoked flag.
2. In exchange, they receive wrapped units of a Token-2022 asset ("leash-wrapped USD") in a token account they hold directly — no custody, no intermediary.
3. That mint carries a **Transfer Hook**: every transfer of the wrapped asset is routed through the `leash-hook` program *before* it can complete. The hook checks the source's Capability — cap, expiry, allowlist, revoked, and (as of Week 3) one level of parent-revocation — and only then allows the transfer to proceed.
4. A capability can be **attenuated**: a parent capability can mint a smaller, narrower, earlier-expiring child capability to a sub-agent, without ever exposing the parent's full budget.
5. **Revocation** is one instruction, one flag flip. Every subsequent spend attempt against that capability (or, one level down, against its children) is rejected on the very next transaction — not eventually, not after a poll interval.

The token program itself is the enforcement point. There is no server to be down, compromised, or lied to.

## Why Solana

The Transfer Hook Interface is a Token-2022-specific primitive: enforcement logic that the token program itself calls on every transfer, with no equivalent in other production token standards. Combined with Solana's local fee markets (one congested account doesn't stall revocation for everyone else) and PDA-based account derivation (a capability's authority is provably a subset of its parent's, checked by the program, not by convention), this is the reason the mechanism exists at all rather than being bolted on top of an existing chain. See [`docs/BUILD_PLAN.md`](docs/BUILD_PLAN.md) §5 for the specific design decisions this rests on, and what was actually validated (not assumed) about each one.

## Architecture

Two Anchor programs:

| Program | Responsibility |
|---|---|
| `leash-program` | Owns `Capability` accounts. Instructions: `issue`, `attenuate`, `revoke`, `redeem`, and `record_spend` (a CPI entrypoint that only `leash-hook` can call). |
| `leash-hook` | A Token-2022 Transfer Hook. Validates cap/expiry/allowlist/revoked — including the full ancestor chain — on every transfer of the wrapped asset, then commits the spend via CPI back into `leash-program`. |

The split exists because Solana's ownership model requires it: `leash-hook` can *read* a Capability account to validate a spend, but only `leash-program` — the account's owner — is allowed to *write* to it. `leash-hook` never holds funds and never mints; `leash-program` never checks a spend.

```
programs/
  leash-program/   Capability state, issue/attenuate/revoke/redeem/record_spend
  leash-hook/      Token-2022 Transfer Hook enforcement
sdk/ts/            TypeScript SDK (@leash/sdk) — typed wrappers for every instruction
cli/               CLI (@leash/cli) — `leash init|mint|attenuate|spend|revoke|redeem|watch`
docs/
  BUILD_PLAN.md    Full spec: data model, instruction set, design decisions, weekly results
leash.txt          Original pitch and long-term vision
```

## Status

**All 6 weeks of the MVP plan are complete**, with real logic and passing tests — not stubs:

- ✅ **Week 1** — Validated that the core mechanism works at all: a real Token-2022 mint with a Transfer Hook, real extra-account resolution, a real transfer confirmed (via on-chain logs) to invoke the hook.
- ✅ **Week 2** — `issue` and `redeem`: a full deposit → issue → redeem round trip, with every token balance and every `Capability` field verified against real account state.
- ✅ **Week 3** — `attenuate` and real spend enforcement: a genuine Token-2022 transfer is checked against cap, expiry, allowlist, and revoked (including one ancestor level), and rejected or allowed accordingly. (Rejections are asserted as failures, not by specific error code — see [`docs/ROADMAP.md`](docs/ROADMAP.md) 0.5.)
- ✅ **Week 4** — The ancestor check extended to the full chain (a depth-3 capability's spend is rejected if *any* of its three ancestors is revoked, each verified independently), `attenuate`'s cap/depth boundary rejections, and the expiry boundary. Every item in the invariant checklist is closed.
- ✅ **Week 5** — TypeScript SDK and CLI, proven against a real `solana-test-validator`: `leash init` → `leash mint` → `leash spend` (succeeds, hook-enforced) → fetch/decode confirms the spend landed on-chain → `leash revoke` → `leash spend` again, and it genuinely fails on-chain.
- ✅ **Week 6** — Both programs deployed to **devnet** (`leash-program`: [`Gbx7nEL2rxWUTj7LnqRQtBDU7yi8oF3miYmjKGncsDXk`](https://explorer.solana.com/address/Gbx7nEL2rxWUTj7LnqRQtBDU7yi8oF3miYmjKGncsDXk?cluster=devnet), `leash-hook`: [`9WPQUY6zVRwVZ3eUsDF1aNESWAyZwL8GwKpzd2C66xtS`](https://explorer.solana.com/address/9WPQUY6zVRwVZ3eUsDF1aNESWAyZwL8GwKpzd2C66xtS?cluster=devnet)). The §11 demo script was run for real against them via the CLI: a $5-capped capability hard-stopped at the cap, and a revoked capability rejected on its very next spend attempt — every step a real, checkable devnet transaction. See `docs/BUILD_PLAN.md`'s "Week 6 results" for every signature.

Full details, including the specific bugs found and fixed along the way at each stage, are in [`docs/BUILD_PLAN.md`](docs/BUILD_PLAN.md) under each week's "results" section.

**Deployed to devnet only. Do not send real (mainnet) funds to any address in this repository.**

## Building and testing

Requires a current Solana CLI / Agave toolchain (platform-tools new enough to support `edition2024` dependencies — see `docs/BUILD_PLAN.md` §5 if you hit a toolchain-version error) and Rust via `rustup`.

```bash
# Build both programs to real, deployable .so binaries
cargo-build-sbf --manifest-path programs/leash-program/Cargo.toml
cargo-build-sbf --manifest-path programs/leash-hook/Cargo.toml

# Run the full test suite (LiteSVM — real compiled programs, real transactions,
# no mocks) across the whole workspace
cargo test --workspace
```

11 tests currently pass: two unit sanity checks, a full deposit/issue/redeem round
trip, the attenuate + spend-enforcement suite, and the Week 4 suite covering the full
ancestor chain, `attenuate`'s boundaries, and the expiry check.

```bash
# Build the SDK, then the CLI (which depends on it via a local file: reference)
cd sdk/ts && npm install && npx tsc && cd ../../cli && npm install && npx tsc

# Against a local validator with both programs loaded (see docs/BUILD_PLAN.md
# for --bpf-program flags), or devnet once deployed:
node dist/index.js -k <keypair.json> init --deposit-mint <mint>
node dist/index.js -k <keypair.json> mint --budget 1000 --expires <unix-ts>
node dist/index.js -k <keypair.json> spend --to <destination-token-account> --amount 100
node dist/index.js -k <keypair.json> watch
node dist/index.js -k <keypair.json> revoke
```

## Explicit non-goals for the current phase

Sharded budgets, ZK/state compression, merkle-proof allowlists, velocity limits, confidential transfers, secp256r1/passkey signing, framework adapters, an `x402` integration, a dashboard, or a hosted service. These are part of the long-term vision (see `docs/BUILD_PLAN.md` §12–13) but deliberately out of scope until the core mechanism is fully proven.

## License

MIT.
