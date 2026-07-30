# Leash — Build Plan (Phase 1: MVP)

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
| 1 ✅ | Spike D1-D4 resolved with working devnet transactions (mint w/ hook attached, a stub hook that logs and passes, a throwaway mint-authority test). No product logic yet — just proof the primitives compose. | "Architecture validated" |
| 2 ✅ | `Capability` account + `issue` + `redeem` instructions. Devnet: deposit USDC, get a wrapped budget, redeem it back. | "Program issues and redeems a real budget" |
| 3 ✅ | `attenuate` + the real `leash-hook` spend-path logic (cap/expiry/allowlist/revoked, single ancestor level). | "Spend is enforced on-chain, not in app code" |
| 4 ✅ | Ancestor-chain checks to `MAX_DEPTH`, full invariant/fuzz suite for all six lines in §2, published as an explicit checklist in the repo. | "Invariants fuzz-tested" |
| 5 ✅ | Minimal TS SDK + CLI (`leash mint --budget X --expires Y --allow Z`). | "10-minute time-to-first-value" |
| 6 ✅ | Devnet deployment, demo script run for real, README/LICENSE polish, publish repo, export AI session transcript, submit grant application. | "Public demo + reproducible proof of work" |

Adjust week numbers to a real calendar date once you set a deadline — this
table is meant to drop directly into the grant form's "Goals and Milestones"
field.

**Suggested Primary KPI for the grant form:** "Working devnet demo: an agent
with a $5 capability is hard-stopped by the token program at $5.01, and a
revoked capability rejects its next spend within one confirmed transaction" —
concrete, binary, checkable by a reviewer without trusting your word for it.

### Week 2 results (2026-07-28) — issue and redeem are real, not stubs

`programs/leash-program/tests/week2_issue_redeem.rs` proves a full round trip
against actual instructions: `issue` transfers a deposit into the vault
(legacy SPL Token — real USDC isn't Token-2022, per D1), creates the
capability's wrapped-token account (an ATA the principal holds directly, not
something leash-program gates access to), mints the wrapped units via a
program-authority PDA, and initializes the root `Capability`. `redeem` burns
wrapped units and withdraws the deposit back out, signed by the same PDA. All
four token balances and every `Capability` field are asserted, not just "the
transaction didn't error."

One design point worth recording: `issue`/`redeem` never touch a *transfer*
of the wrapped mint — only `mint_to`/`burn` — so `leash-hook`'s TransferHook
is never invoked by either instruction, and Week 2's test doesn't need
`leash-hook` loaded at all. The hook only fires on `spend` (Week 3). A single
shared PDA (`AUTHORITY_SEED`) serves as both the wrapped mint's mint
authority and the vault's token-account authority — one authority instead of
two, since nothing here requires them to be separate for the MVP.

Note what's still fully open: the wrapped mint and vault are still created
off-chain/client-side (as in the Week 1 spike), not via an on-chain
"initialize deployment" instruction — `issue`/`redeem` operate against an
already-configured deployment. That's a reasonable MVP simplification, not a
gap to silently forget: a real deployment needs that setup scripted
somewhere before Week 5's CLI can `leash mint` against it.

### Week 3 results (2026-07-28) — real enforcement, not just plumbing

`programs/leash-program/tests/week3_spend_enforcement.rs` proves the actual
point of the project: a real Token-2022 transfer of the wrapped asset is
checked against cap, expiry, allowlist, and revoked — including one ancestor
level — and rejected or allowed accordingly, not just logged. An earlier draft
of this test had two assertions that "passed" for the wrong reason
(`AlreadyProcessed`, because the retry transaction was byte-for-byte
identical to one already submitted, not because leash-hook rejected it);
caught by reading the failure reason during development, fixed by varying
the amount so each transaction is genuinely distinct.

> **Correction (2026-07-29).** As originally written, this paragraph claimed
> every rejection "was verified by its actual on-chain error code, not just
> `is_err()`." That overstated the committed tests and is corrected above.
> Failure reasons were read *manually, during development* — that is how the
> `AlreadyProcessed` artifacts were caught, and that fix is real and committed.
> But the assertion itself is `expect_err` (`tests/common/mod.rs:58-61`), which
> only checks that the call failed; it prints the error rather than inspecting
> it. There are no error-code assertions anywhere in the suite. Tracked as
> docs/ROADMAP.md 0.5, which also notes this is why the cap-vs-balance
> ambiguity below is invisible to the tests.

Three design points this week surfaced, none of which were obvious from D1-D4:

- **`Capability.parent` had to change from `Option<Pubkey>` to a sentinel
  `Pubkey`** (`Pubkey::default()` for "no parent"). Borsh encodes
  `Option<T>` as a 1-byte tag plus 32 bytes only when `Some`, so its on-disk
  size — and the offset of every field after it — would shift depending on
  whether a capability has a parent. leash-hook needs `parent` at a fixed
  byte offset to read it directly out of raw account data via
  `PubkeyData::AccountData` (see below); that's incompatible with a
  variable-size field. See `state::PARENT_FIELD_OFFSET`.
- **`attenuate`'s child seed scheme had to match `issue`'s root scheme
  exactly** — both are now `[CAPABILITY_SEED, owner]`, not
  `[CAPABILITY_SEED, parent, owner]` as originally drafted. leash-hook can
  only derive "the Capability for this transfer" using one fixed seed
  formula, registered once at mint-creation time, from the transfer's own
  accounts (specifically the token account's owner). Two different
  derivation schemes for root vs. child capabilities can't both be that one
  formula. Consequence, documented rather than hidden: one owner can hold at
  most one active capability (root or child) at a time.
  <br>**[Superseded — see docs/ROADMAP.md 0.3.]** The single-formula constraint
  still holds and is still the reason root and child must share a scheme, but
  the formula is no longer keyed on the owner: it is `[CAPABILITY_SEED, <the
  capability's own token account>]`, resolved by the hook from base account 0
  instead of base account 3. The one-capability-per-owner consequence recorded
  above is therefore no longer true of the current code.
- **leash-hook cannot write `spent` itself.** Capability accounts are owned
  by leash-program; Solana only lets the owning program mutate an account's
  data. leash-hook validates everything (cap/expiry/allowlist/revoked/parent)
  read-only, then commits the spend via CPI into a new `record_spend`
  instruction on leash-program. Access control on that CPI took a real bug to
  get right: `hook_authority` was originally typed as a plain
  `UncheckedAccount`, which made Anchor's generated CPI instruction mark
  `is_signer: false` in its account metas regardless of `invoke_signed` —
  the fix was adding `#[account(signer)]` to the field. Found by seeing
  `is_signer == false` inside `record_spend` with an otherwise-correctly-matching
  PDA address, not by guessing.

Also confirmed via the chained-seed-resolution research this week (not
assumed): `spl_tlv_account_resolution`'s `Seed`/`PubkeyData` system supports
resolving one extra account's address from *another already-resolved extra
account's* data, which is what makes "look up the source's Capability, then
look up *that* Capability's parent" possible as two chained, generically
client-resolvable extra accounts rather than something leash-hook has to
special-case.

Honest gap in the test, not swept under the checkmark: the "spend exceeding
cap" rejection is observably identical to what Token-2022's own
insufficient-balance check would produce, since this MVP always mints
exactly `cap` wrapped units to a capability's account — nothing currently
lets token balance and remaining cap diverge. The rejection is real, but the
test doesn't isolate *which* of the two overlapping mechanisms caught it.

Still deferred to Week 4, per the original plan: ancestor checks beyond one
level (to `MAX_DEPTH`), and the full invariant/fuzz suite.

### Week 4 results (2026-07-28) — full ancestor chain + invariant checklist complete

Extending the ancestor check from one level to the full `MAX_DEPTH` chain
surfaced a real design flaw in the Week 3 approach, caught by actually
running the extended version, not by inspection: chaining each ancestor
account off the *previous* ancestor's own `parent` field breaks the moment
an early ancestor turns out to be the root placeholder. A root capability's
`parent` resolves to `Pubkey::default()` — the System Program's address by
convention — which has **zero account data**, so trying to read a "further
parent" out of that empty data fails client-side, during account
resolution, before a transaction is even submitted (`AccountDataTooSmall`,
surfaced as an opaque `Custom(2724315855)`).

The fix is more robust than patching around the failure: `Capability` now
carries its **entire ancestor chain directly** —
`ancestors: [Pubkey; ANCESTOR_SLOTS]` (`ANCESTOR_SLOTS = MAX_DEPTH = 3`),
populated by `attenuate` as `[parent, parent.ancestors[0],
parent.ancestors[1]]`. leash-hook's extra-account-meta config now reads all
three ancestor slots as **fixed offsets directly out of the source
capability's own data** (`ANCESTORS_FIELD_OFFSET`, `+32`, `+64`), never by
chaining through another account's data. Since the source capability being
spent from always has real, fully-populated data, this can never hit the
empty-account problem the chained version did. `spend_logic`'s ancestor loop
itself didn't need to change — it already walked `accounts[7 + level] for
level in 0..capability.depth`; only *how those accounts got there* changed.

`programs/leash-program/tests/week4_ancestor_chain_and_fuzz.rs` closes every
remaining item on the invariant checklist (`tests/invariants/README.md`,
now fully checked): a depth-3 chain (root → A → B → C) with revocation
tested at each of the three ancestor levels independently (each in its own
fresh chain, so revoking the wrong level can't accidentally make a test
pass), plus the positive case where nothing is revoked; `attenuate`
rejecting a child cap exceeding the parent's remaining budget (and accepting
exactly at the boundary); `attenuate` rejecting depth past `MAX_DEPTH`; and
the expiry boundary (before succeeds, one second after fails, using
LiteSVM's `set_sysvar::<Clock>` to warp time rather than waiting).

Also refactored: the three test files' shared setup/issue/attenuate/revoke/
spend helpers were pulled into `tests/common/mod.rs` (Cargo's native
shared-test-module convention) rather than staying duplicated per file —
Week 4 needed several of them unchanged, and copy-pasting them a third time
would have been the wrong call.

What "fuzz suite" turned out to mean in practice: a specific, hand-built
test per checklist line, not generated/randomized fuzzing. That distinction
is now stated explicitly in `tests/invariants/README.md` rather than left
ambiguous — genuine property-based fuzzing (e.g. via `trident` or `proptest`
over instruction sequences) is a real gap if deeper assurance is ever needed
before real funds touch this program, not something this week's suite
should be mistaken for.

### Week 5 results (2026-07-28) — TS SDK + CLI, proven against a real validator

`sdk/ts/` (`@leash/sdk`) wraps every instruction (`mint`/`issue`, `attenuate`,
`revoke`, `redeem`, `spend`, `watch`/`fetchCapability`) plus deployment setup
(`createDeployment`: vault + wrapped Token-2022 mint + hook registration,
still client-side per the Week 2 note). `cli/` (`@leash/cli`) is a thin
`commander` wrapper: `leash init|mint|attenuate|spend|revoke|redeem|watch`.

Deliberately not just type-checked: the whole loop was run against a real
`solana-test-validator` (both programs loaded via `--bpf-program`, not
LiteSVM, since LiteSVM has no JS binding used in this session) —
`leash init` → `leash mint` (issued a real capability on-chain) → `leash
spend` (a real Token-2022 transfer, hook-enforced, succeeded) → fetch the
capability and confirm `spent` actually incremented on-chain → `leash revoke`
→ `leash spend` again, and confirmed it **genuinely fails on-chain**
(`AnchorError ... Error Code: Revoked`, thrown from inside leash-hook during
Token-2022's own `TransferChecked`, not a client-side precheck). This is the
same proof leash.txt's own demo script (§11) asks for, just exercised via the
CLI instead of a screen recording.

Four real bugs found this week, none obvious in advance:

- **Anchor's JS coder lowercases the first letter of account names.** The
  Rust struct is `Capability`, and the raw IDL JSON says `"name":
  "Capability"`, but a live `Program` instance's `coder.accounts.decode(...)`
  only recognizes `"capability"` — confirmed by hitting `Account not found:
  Capability` against a real `Program` instance, then inspecting
  `program.idl.accounts` directly, not assumed from convention. Fixed in
  `sdk/ts/src/watch.ts`.
- **`connection.sendTransaction`/raw `Transaction` objects don't set
  `feePayer`/`recentBlockhash` for you.** Omitting them doesn't fail at
  compile time — it fails at simulation with a cryptic "Attempt to debit an
  account but found no record of a prior credit." Fixed by explicitly setting
  both before signing in `deployment.ts` and `spend.ts` (`sendAndConfirm`
  helper).
- **`anchor build`'s IDL generation lint is stricter than plain
  `cargo-build-sbf`.** It rejects `UncheckedAccount` fields missing a `///
  CHECK:` doc comment even though the program itself builds fine without one
  — caught two such fields in `attenuate.rs` (`token_2022_program`,
  `associated_token_program`) once the SDK's IDL-driven workflow needed a
  real `anchor build` for the first time.
- **A stale ledger from a prior failed attempt produced a misleading crash.**
  Before the feePayer fix above, a failed `leash init` left corrupted
  intermediate state on the local validator's ledger. The *next* attempt
  (already carrying the fix) then crashed inside leash-hook's
  `initialize_extra_account_meta_list` with "Access violation ... address
  0x4" — despite every LiteSVM test for that exact function passing. Traced
  by adding temporary `msg!("checkpoint: ...")` lines to bisect, but the real
  fix turned out to be simpler: restarting the validator with `--reset` (a
  genuinely fresh ledger) made the crash disappear immediately, confirming it
  was inherited state, not a Rust bug. Checkpoints were removed afterward and
  the entire flow was rerun from a truly fresh ledger to get a non-lucky
  confirmation.

Also worth recording as an environment quirk, not a code bug: this
sandbox's conventional default keypair path (`~/.config/solana/id.json`,
the CLI's sensible default) has zero balance — the actually-funded keypair
is `~/.config/solana/server-keypair.json`. All testing above used `-k
~/.config/solana/server-keypair.json` explicitly rather than changing the
CLI's default.

### Week 6 results (2026-07-28) — real devnet deployment, demo script run for real

Both programs are deployed to devnet, upgradeable, under the same authority as the
already-committed program keypairs (no ID changes needed):

- `leash-program`: [`Gbx7nEL2rxWUTj7LnqRQtBDU7yi8oF3miYmjKGncsDXk`](https://explorer.solana.com/address/Gbx7nEL2rxWUTj7LnqRQtBDU7yi8oF3miYmjKGncsDXk?cluster=devnet)
  — deploy tx [`GqfKnfDZE2MWdaEqSEAP8cGx6BT7zGrTJyunedqERkVztDbivi9ZZ79BgeF52DcNksfwsaVhmyesnbYsdNsZ2Rr`](https://explorer.solana.com/tx/GqfKnfDZE2MWdaEqSEAP8cGx6BT7zGrTJyunedqERkVztDbivi9ZZ79BgeF52DcNksfwsaVhmyesnbYsdNsZ2Rr?cluster=devnet)
- `leash-hook`: [`9WPQUY6zVRwVZ3eUsDF1aNESWAyZwL8GwKpzd2C66xtS`](https://explorer.solana.com/address/9WPQUY6zVRwVZ3eUsDF1aNESWAyZwL8GwKpzd2C66xtS?cluster=devnet)
  — deploy tx [`55xUEB4LWMsoJfmhf7a2VNCRSMUmdPTcCroiZ2uudyyQ56fis5UYBxcDJaHWZ8j3ctiLEk8X89jQnTF397WAPiH2`](https://explorer.solana.com/tx/55xUEB4LWMsoJfmhf7a2VNCRSMUmdPTcCroiZ2uudyyQ56fis5UYBxcDJaHWZ8j3ctiLEk8X89jQnTF397WAPiH2?cluster=devnet)

The full §11 demo script was then run for real against these deployed programs — via
the CLI, not LiteSVM, not localnet — using a mock devnet SPL Token as the deposit asset
(`8NYUrHgv3Grxvw7KTzSrq84q1bvWMLgZtuoR4auehw2`; real USDC works identically per D1, a
mock mint just avoids needing devnet USDC specifically). Every step below is a real,
independently-verifiable devnet transaction, not a log line:

**Cap enforcement** — a capability issued with `budget=5, expiry=+1h`, allowlisted to a
merchant token account:
- Issue: [`5AT66HmhW2DyWT2p17Fu3fcN7uzFrAhnvdJi71SMjSpYDHfBgXVgufWqJa7dM3YeX2ggEkbN9UKNYeEfKGworY8E`](https://explorer.solana.com/tx/5AT66HmhW2DyWT2p17Fu3fcN7uzFrAhnvdJi71SMjSpYDHfBgXVgufWqJa7dM3YeX2ggEkbN9UKNYeEfKGworY8E?cluster=devnet)
- Spend 3 of 5 (succeeds): [`4zytrBZTQ24kPrx7FwKGUssoLhGRdBsn6dHE3PiLKorhEcwMmczxgx4Rpu8YVULFG6H28GFqPhcpLUvafBByyMdG`](https://explorer.solana.com/tx/4zytrBZTQ24kPrx7FwKGUssoLhGRdBsn6dHE3PiLKorhEcwMmczxgx4Rpu8YVULFG6H28GFqPhcpLUvafBByyMdG?cluster=devnet)
- Spend 3 more (would total 6 > cap of 5): rejected in simulation, never lands a
  signature — `custom program error: 0x1` ("insufficient funds"), thrown by Token-2022
  itself. As already noted honestly in the Week 3 results, this MVP always mints
  exactly `cap` wrapped units to a capability, so a cap-exceeded spend and a
  balance-exceeded spend are the same observable failure here — the rejection is real
  and matches §11's literal ask ("show the transfer fail at the token-program level,
  not an application error"), it just doesn't isolate which of the two overlapping
  checks caught it. Still an open item for later, not fixed this week.

**Instant revocation** — a second capability (different owner, since one owner holds at
most one capability by design — see Week 3 results), `budget=10`:
- Issue: [`goQbtHjVbzRdUwRnW3wBb1x6f3XFqQ6tM1BLX189azMXjN81dU3vPGjcXCHGrkEGkZK8oGmc7XyaNkhVDNPcXDU`](https://explorer.solana.com/tx/goQbtHjVbzRdUwRnW3wBb1x6f3XFqQ6tM1BLX189azMXjN81dU3vPGjcXCHGrkEGkZK8oGmc7XyaNkhVDNPcXDU?cluster=devnet)
- Spend 4 (succeeds): [`4oiSYikE5L7yHdGBiwFQMRnwxE6yiaGNujQgqJNippGgTyAo2oi5U27DDJEaB8BXJF4WSQwuBkvWBCzu6Nebv7Dr`](https://explorer.solana.com/tx/4oiSYikE5L7yHdGBiwFQMRnwxE6yiaGNujQgqJNippGgTyAo2oi5U27DDJEaB8BXJF4WSQwuBkvWBCzu6Nebv7Dr?cluster=devnet)
- Revoke: [`5fHBj1vs4gYUS6Ygd5v1NcDrfd4Nm5yjT6m3Hfm4tN4Ad2XpjovPCbvBieMgUj5ZuHkbpM1PqrJYkXtrqrUegNVo`](https://explorer.solana.com/tx/5fHBj1vs4gYUS6Ygd5v1NcDrfd4Nm5yjT6m3Hfm4tN4Ad2XpjovPCbvBieMgUj5ZuHkbpM1PqrJYkXtrqrUegNVo?cluster=devnet)
- Spend 1 (next attempt): rejected in simulation, never lands a signature —
  `AnchorError thrown in programs/leash-hook/src/lib.rs:204. Error Code: Revoked`,
  thrown from inside `leash-hook` during Token-2022's own `TransferChecked`. This one
  *does* cleanly isolate leash-hook's own check — nothing about revocation touches
  token balance, so there's no ambiguity with a Token-2022-level failure the way the
  cap case above has.

No devnet SOL rate-limit issues blocked this beyond one airdrop request being
throttled (worked around by reusing the already-funded balance from earlier local
testing — nothing here required real/mainnet funds). Total devnet spend: ~2.66 SOL in
program rent-exemption (refundable if either program is ever closed) plus a handful of
transaction fees, all devnet-faucet SOL.

Added `LICENSE` (MIT), matching what `README.md`'s License section already stated —
the file itself hadn't existed until now.

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
