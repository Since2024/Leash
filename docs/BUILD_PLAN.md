# Leash — Build Plan (Phase 1: Grant-Scoped MVP)

Source pitch: `../leash.txt`. This document narrows that pitch into something one
person can actually build and demo in about a month, using AI-agent-assisted
development. It exists to be followed literally, not admired.

## 0. What this is, in plain English

An agent holds money to spend on your behalf. Instead of trusting the agent's
code to respect a limit, the limit is enforced by the token itself: every
transfer of "leash dollars" runs through a program that checks the budget,
the expiry, and the allowed destinations, and refuses the transfer if any of
them are violated — not with an error message the caller could ignore, but by
making the transaction itself invalid. Revoking access flips one flag and
every future transfer from that budget fails immediately, everywhere.

This MVP proves exactly that mechanism, at the smallest scale that's still
real: one budget, no sub-agents yet, no sharding, no compression. If this
doesn't work, nothing later in the vision works either — so it's the correct
first thing to build.

## 1. Scope of this document

**In scope (Phase 1, this doc):** a single-shard, devnet-only Anchor program
+ Token-2022 transfer hook that can issue a capped, expiring, allowlisted
budget; spend against it; attenuate it into one child budget; and revoke it
instantly. Plus a minimal SDK, CLI, fuzz tests, and a demo.

**Explicitly out of scope for Phase 1** (see §10): sharded budgets, ZK/state
compression, merkle-proof allowlists, velocity limits, confidential
transfers, framework adapters, `leash-verify` middleware, x402 integration,
mainnet deployment, any hosted service. These are real and are listed at the
end (§13) so they aren't forgotten, not because they're unimportant.

## 2. Non-negotiable properties (what "done" must satisfy)

1. A capability cannot spend more than its `cap`.
2. A capability cannot spend after its `expiry`.
3. A capability cannot pay a destination not on its allowlist.
4. A revoked capability — or one whose parent is revoked — cannot spend,
   within one confirmed transaction of the revoke call.
5. A child capability's cap can never exceed its parent's remaining,
   un-committed budget, enforced by the program, not by convention.
6. None of the above are enforced by an off-chain service. If the enforcing
   program is unreachable, spends fail closed (no spend goes through), they
   don't fail open.

Everything below is in service of these six lines. If a design decision
threatens one of them, stop and reconsider the design, don't relax the line.

## 3. Data model

**Mint**: one Token-2022 mint per deployment, `TransferHook` extension
pointing at `leash-hook`. Represents "leash-wrapped USD," pegged 1:1 to real
USDC held in a vault. (Real USDC on Solana is a plain SPL Token mint, not
Token-2022 — you cannot attach a hook to it directly. Wrapping is required;
see D1.)

**Vault**: one PDA token account (plain USDC, owned by `leash-program`) per
deployment, or per-principal if simpler for MVP. Holds the real money.
Redemption is a separate, explicit instruction (see D2) — not something the
hook does.

**Capability account** (Anchor account, one per issued or attenuated
budget):

```rust
pub struct Capability {
    pub owner: Pubkey,            // signer who can attenuate/revoke this node
    pub parent: Option<Pubkey>,   // None for root capabilities
    pub token_account: Pubkey,    // the Token-2022 account holding this capability's balance
    pub cap: u64,                 // total this capability may ever spend
    pub spent: u64,                // cumulative amount spent so far
    pub committed_to_children: u64, // sum of caps handed to attenuated children
    pub expiry: i64,               // unix timestamp
    pub allowlist: Vec<Pubkey>,     // MVP: flat list, max ~10 entries, not a merkle root
    pub revoked: bool,
    pub depth: u8,                  // 0 for root; capped at MAX_DEPTH (suggest 3) for MVP
}
```

Invariant checked on every mutation: `spent + committed_to_children <= cap`.

## 4. Instructions

### `issue(cap, expiry, allowlist)`
Principal deposits `cap` USDC into the vault. Program mints `cap` units of
leash-wrapped-USD to a fresh token account. Initializes a root `Capability`
(`parent = None`, `depth = 0`, `spent = 0`, `committed_to_children = 0`,
`revoked = false`).

### `attenuate(parent_capability, child_cap, child_expiry, child_allowlist)`
Does **not** go through the transfer hook — it's a mint, not a transfer (see
D3 for why). Checks: caller owns `parent_capability`; `child_cap <=
parent.cap - parent.spent - parent.committed_to_children`; `child_expiry <=
parent.expiry`; `child_allowlist` is a subset of `parent.allowlist` (or equal,
for MVP simplicity — don't build arbitrary set-narrowing logic yet, just
require equality or a smaller explicit list); `parent.depth < MAX_DEPTH`.
On success: mints `child_cap` units to a new token account, creates the child
`Capability` (`parent = Some(parent_pubkey)`, `depth = parent.depth + 1`),
increments `parent.committed_to_children`.

### `spend` — enforced entirely inside the Token-2022 transfer
This is a normal token transfer from a capability's token account to a
destination. The mint's `TransferHook` extension routes it through
`leash-hook` before it can complete. The hook:
1. Loads the source token account's associated `Capability` (passed as an
   extra account, per the Transfer Hook Interface's `ExtraAccountMetaList`).
2. Loads the capability's ancestor chain up to `MAX_DEPTH` (also passed as
   extra accounts — bounded and known at attenuation time).
3. Rejects if `revoked` is true on the capability **or any ancestor**.
4. Rejects if `now > expiry`.
5. Rejects if `destination not in allowlist`.
6. Rejects if `amount + spent > cap`.
7. Otherwise increments `spent` on the source capability and allows the
   transfer.

Note the asymmetry with `attenuate`: attenuation mints new supply to a child
account and is authorized by the program directly; spending moves existing
supply to an external destination and is gated by the hook. This is what
lets the hook logic stay simple — it only ever has to reason about "is this
transfer a spend," never "is this transfer secretly an attenuation."

### `revoke(capability)`
Caller must own the capability (or an ancestor of it, if you want
owner-level cascading revoke — start with "only the exact owner of this
node," extend later). Sets `revoked = true`. No token movement. Takes effect
on the very next transfer attempt against that capability or any of its
descendants, because the hook walks the ancestor chain on every spend.

### `redeem` (merchant-side, minimal)
Anyone holding leash-wrapped-USD can redeem it 1:1 for real USDC from the
vault. Simple burn-and-withdraw. Exists so accepting a Leash payment is as
good as accepting cash — the merchant doesn't have to trust anything beyond
the peg.

## 5. Design decisions to validate early (spike in Week 1, not Week 4)

- **D1 — Wrapping is required.** Native USDC is a plain SPL Token mint; you
  cannot retrofit a `TransferHook` onto it. Confirm this against current
  Token-2022/SPL docs before writing any other code — if it's wrong, the
  whole architecture changes.
- **D2 — The hook cannot move the vault's real USDC itself.** Token-2022
  transfer hooks receive source/destination accounts as read-only and run in
  a restricted CPI context. This is why redemption is a separate, explicit
  instruction rather than something the hook triggers automatically. Confirm
  this constraint directly against the current Transfer Hook Interface
  before assuming it.
- **D3 — Attenuation is a mint, not a transfer.** This sidesteps the need for
  the hook to distinguish "spend" from "internal budget move." Validate that
  the Token-2022 mint authority model actually lets the program mint new
  supply post-issuance the way this assumes (mint authority = program PDA).
- **D4 — Ancestor-chain revocation via extra account metas.** The Transfer
  Hook Interface's extra-account-meta mechanism must support passing a
  bounded, capability-specific set of ancestor accounts into the hook call.
  If `ExtraAccountMetaList` can't express "look up N accounts derived from
  this specific capability's stored ancestor pubkeys," the depth-checking
  design needs to change (e.g., cap `MAX_DEPTH` at 1, or store a
  root-revoked flag directly on every descendant instead of walking a chain).

If any of D1-D4 turns out false, stop and redesign before writing the fuzz
suite — don't build on top of an unvalidated assumption.

### Week 1 spike results (2026-07-28) — D1-D4 all hold

Validated with a real LiteSVM test
(`programs/leash-hook/tests/spike_d1_d4.rs`): a fresh Token-2022 mint with the
`TransferHook` extension, a real `initialize_extra_account_meta_list` call,
and a real `transfer_checked` that triggers the hook and is confirmed via its
own on-chain logs (`leash-hook: spike_execute invoked...`), followed by a
second `mint_to` standing in for `attenuate`. All four hold as originally
assumed. What the spike also surfaced, none of which was obvious in advance:

- **Anchor's `#[program]` macro does not dispatch `Execute` directly.**
  Token-2022 CPIs into the hook using the Transfer Hook Interface's own fixed
  8-byte discriminator (`SplDiscriminate`), not Anchor's sighash. The bridge
  is Anchor's `fallback(program_id, accounts, data)` entrypoint, which
  manually unpacks `TransferHookInstruction` and dispatches to a plain shared
  function — not through Anchor's generated `__private::__global` dispatcher,
  which ties instruction-data lifetimes to the accounts slice in a way a
  freshly-encoded local byte array can't satisfy on the current anchor-lang
  (confirmed by hitting real E0621/E0597 errors, not by inspection).
- **A freshly-derived PDA with `allocate`+`assign` and no lamports transfer
  silently disappears.** `system_instruction::allocate` sets data size but
  moves no lamports; any account at 0 lamports at the end of a transaction is
  reclaimed by the runtime regardless of its data. `initialize_extra_account_
  meta_list` must fund the PDA to rent-exemption before allocate/assign — the
  reference implementation this was adapted from has this same gap. Found by
  the PDA reading back as `None` after a "successful" init transaction, not
  by inspection.
- **Real USDC cannot take a `TransferHook` extension (D1, confirmed
  directly)**: it's a plain SPL Token mint. The wrapped-asset design in §3 is
  required, not a simplification.
- **The hook only ever reads accounts (D2, confirmed directly)**: nothing in
  `spike_execute_logic` moves tokens; Token-2022 owns the actual transfer.
  `redeem` staying a separate explicit instruction (§4) is the right call.
- **Dependency version pinning is load-bearing, not cosmetic.** Adding
  `litesvm`, `spl-token-2022`, or `spl-associated-token-account-client` at
  their latest versions (rather than pinned to what `anchor-lang`/`anchor-spl`
  1.1.2 already resolve) pulled in a second, incompatible generation of
  `solana-instruction`/`solana-pubkey`/`wincode` and cost real time to trace.
  Fix: pin dev-dependencies to match versions already in the main dependency
  tree; reuse `anchor_spl::token_2022::spl_token_2022` and
  `anchor_spl::associated_token::spl_associated_token_account` re-exports
  instead of adding parallel SPL crates directly.
- **The installed Solana CLI toolchain was too old for the current crate
  ecosystem.** 1.18.26's bundled platform-tools shipped `rustc 1.75.0`, which
  can't parse a `edition2024`-requiring dependency or a Cargo.lock v4. Fixed
  by upgrading via the official installer
  (`https://release.anza.xyz/stable/install`) to Agave 4.1.1 / platform-tools
  v1.54 — after which both lockfile formats and edition2024 dependencies work
  fine. Anyone else building this from a stale Solana CLI install will hit
  the same wall.

D4's dynamic-per-source-account derivation (a real ancestor Capability PDA
instead of the spike's fixed placeholder account) is still open — this
spike only proved the *mechanism* (extra accounts resolve and arrive
correctly), not the *specific* seed scheme Week 3 needs. That's the next
open question, not a settled one.

## 6. Repo structure

```
leash/
  programs/
    leash-program/     Anchor program: Capability state, issue, attenuate, revoke, vault, redeem
    leash-hook/         Transfer hook program: spend-path enforcement only
  sdk/
    ts/                 mint(), attenuate(), spend(), revoke(), watch()
  cli/                  `leash mint`, `leash attenuate`, `leash spend`, `leash revoke`, `leash watch`
  tests/
    integration/        Anchor test suite (happy paths)
    invariants/          Property-based / fuzz tests for the six lines in §2
  docs/
    BUILD_PLAN.md        this file
  README.md
```

## 7. Milestones (target: ~6 weeks, devnet only)

| Week | Deliverable | Maps to grant KPI |
|---|---|---|
| 1 | Spike D1-D4 resolved with working devnet transactions (mint w/ hook attached, a stub hook that logs and passes, a throwaway mint-authority test). No product logic yet — just proof the primitives compose. | "Architecture validated" |
| 2 | `Capability` account + `issue` + `redeem` instructions. Devnet: deposit USDC, get a wrapped budget, redeem it back. | "Program issues and redeems a real budget" |
| 3 | `attenuate` + the real `leash-hook` spend-path logic (cap/expiry/allowlist/revoked, single ancestor level). | "Spend is enforced on-chain, not in app code" |
| 4 | Ancestor-chain checks to `MAX_DEPTH`, full invariant/fuzz suite for all six lines in §2, published as an explicit checklist in the repo. | "Invariants fuzz-tested" |
| 5 | Minimal TS SDK + CLI (`leash mint --budget X --expires Y --allow Z`). | "10-minute time-to-first-value" |
| 6 | Demo video (see §12), README, MIT license, publish repo, export AI session transcript, submit grant application. | "Public demo + reproducible proof of work" |

Adjust week numbers to a real calendar date once you set a deadline — this
table is meant to drop directly into the grant form's "Goals and Milestones"
field.

**Suggested Primary KPI for the grant form:** "Working devnet demo: an agent
with a $5 capability is hard-stopped by the token program at $5.01, and a
revoked capability rejects its next spend within one confirmed transaction" —
concrete, binary, checkable by a reviewer without trusting your word for it.

## 8. Test & fuzz plan

For each of the six lines in §2, write a test that tries to violate it and
confirms the transaction fails:
- Spend exactly at cap succeeds; spend one unit over fails.
- Spend before expiry succeeds; spend one second after fails.
- Spend to an allowlisted destination succeeds; spend to any other pubkey fails.
- Revoke a capability, then attempt a spend — fails.
- Revoke a *parent*, then attempt a spend from a *child* — fails.
- Attempt to attenuate a child with `cap` greater than the parent's remaining
  budget — fails.
- Attempt to attenuate past `MAX_DEPTH` — fails.

This isn't formal verification — it's the minimum bar given leash.txt's own
warning that "a bug here is a total-loss bug." Treat every test above as
mandatory before touching the CLI or SDK.

## 9. Explicit non-goals for this phase

Do not build, even if it's tempting mid-sprint: sharded budgets or the
voucher-merge design, ZK/state compression, merkle-proof allowlists, velocity
limits, confidential transfers, secp256r1/passkey signing, framework
adapters (OpenAI/LangGraph/CrewAI), `leash-verify` middleware, x402 client
integration, a dashboard, a hosted service, mainnet deployment. Every one of
these is a real later step (§13) and a real distraction right now.

## 10. Risks specific to this phase

- **D1-D4 could be wrong.** That's why they're a Week 1 spike, not a Week 4
  surprise.
- **Real-money temptation.** Stay on devnet for the entire grant-scoped
  phase. Do not deploy to mainnet with real USDC until the fuzz suite in §8
  is complete and, ideally, someone other than the author has read the
  program.
- **Scope creep toward the full vision.** leash.txt's own long-term plan is
  6 months and includes sharding, compression, and a dozen integrations.
  This document is deliberately a fraction of that. Resist expanding it
  before Phase 1 ships.

## 11. Definition of done / demo script

A single ~40-second recording, matching leash.txt's own go-to-market
instinct: mint a capability with `leash mint --budget 5 --expires 1h --allow
<devnet-merchant-pubkey>`; run a loop that spends against it; show the
transfer fail at the token-program level once the cap is hit — not an
application error, the transaction itself rejected; then mint a fresh
capability, spend once successfully, call `leash revoke`, and show the next
spend attempt fail immediately. That video, the public repo, the exported AI
session transcript, and this document together are the grant submission's
proof of work.

## 12. Later phases (not specified in detail yet — deliberately)

- **Sharded budgets + mint-side voucher merge**: needed once a single parent
  is spent against by many concurrent children fast enough to serialize on
  one hot account. Don't design this until Phase 1's single-shard version is
  running and you can feel the actual bottleneck.
- **ZK/state compression**: makes minting thousands of short-lived,
  per-task capabilities cheap. Valuable once usage volume makes per-capability
  rent meaningful — premature before that.
- **Merkle-proof allowlists**: needed once allowlists get too large for a
  flat `Vec<Pubkey>` to be cheap to store/check.
- **Velocity/rate limits**: a real feature, deferred because cap + expiry +
  allowlist already cover the core guarantee.
- **Framework adapters, `leash-verify`, x402 client**: distribution and
  integration work — do this only after the primitive itself is proven,
  per leash.txt's own "don't build the dashboard" instinct.

## 13. Appendix — sources

- `../leash.txt` — original pitch, self-critique, and long-term vision.
- This session's fact-checks: Token-2022 Transfer Hook mechanics (read-only
  account access during hook execution), Solana's native Subscriptions &
  Allowances program (explicitly incompatible with `TransferHook` mints,
  confirming this design doesn't collide with it), and a Colosseum
  builder-project crowdedness check (no close match found for this specific
  mechanism across 5,400+ submissions, as of 2026-07-28).
