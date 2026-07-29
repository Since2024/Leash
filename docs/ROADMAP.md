# Roadmap: from working MVP to something worth trusting with money

`docs/BUILD_PLAN.md` is a closed record. The six-week Phase 1 MVP it specifies is done,
and its "Week N results" entries stay as written — they're a build log, not a plan.

This file picks up where that one stops: the distance between *the demo works* and *a
stranger can put real money behind it*. It is not a company roadmap. No dates, no funding
milestones, no feature marketing — just the correctness, assurance, and ergonomics work
standing between the current devnet deployment and a mainnet one.

**How to read it.** Items are grouped by what they block, not by effort. Phase 0 items
block mainnet outright: each one is a way the six non-negotiable properties in
BUILD_PLAN.md §2 can currently be violated, or a limitation severe enough to make the
primitive unusable. Phase 1 is the assurance work that makes "we think it's correct"
into something defensible. Phases 2–3 are what makes it adoptable. Everything after is
inherited from BUILD_PLAN.md §12 and leash.txt, unchanged and still deliberately vague.

Where an item was found by reading the code rather than inherited from the build plan,
it says so and cites the file. Nothing here is aspirational filler — if it's listed,
there's a specific thing wrong or missing at a specific place.

**Status markers.** Every claim of "done" on this page is checkable against `main`.

- `[ ]` — not started.
- `[~]` — designed, not landed. Code exists on a branch that was never pushed, never
  built, and never tested. Treat as unstarted for any purpose that matters.
- `[x]` — done and present on `main`.

## Status

Phase 1 MVP complete: both programs deployed to devnet, full hook enforcement
(cap/expiry/allowlist/revoked + the ancestor chain to `MAX_DEPTH`), TS SDK and CLI proven
end-to-end against a real validator. Nothing on mainnet, and per BUILD_PLAN.md §10 that
stays true until at minimum Phase 0 and Phase 1 below are closed.

Two of the Phase 0 items below were **critical** — either one, on its own, sufficient to
lose depositor funds, and both reachable without any unusual behaviour. Both (0.1, 0.2)
are now fixed, each covered by tests verified to fail without their fix. The remaining
Phase 0 items are real but none of them lose money on their own.

---

## Phase 0 — Correctness gaps that block mainnet

### 0.1 `redeem` bypassed the capability entirely — **critical**

- [x] **Fixed.** A capability holder could convert its unspent budget into unrestricted
      real USDC.

`redeem` (`programs/leash-program/src/instructions/redeem.rs`) burns wrapped units from
any token account its signer controls and pays out the deposit asset 1:1. It never
references a `Capability` account — not once in the whole file — and burning doesn't
invoke the transfer hook, because Token-2022 hooks fire on transfers, not burns.

The burn is authorized by the holder directly (`authority: holder`, `redeem.rs:52`), and
the payout destination `holder_deposit_account` is an unconstrained `UncheckedAccount`
(`redeem.rs:35`). A capability's token account is controlled by its owner by design — the
capability is a bearer object the holder controls. So a delegated agent holding a $20
child capability can call `redeem` and receive $20 of real USDC at an address of its
choosing.

This defeats property 3 (allowlist) completely for any un-spent balance, and property 4
(revocation) for the same — a revoked or expired capability's tokens are still
redeemable, because nothing on the redeem path reads `revoked` or `expiry`. It directly
contradicts the pitch in `leash.txt`: "Give your agent $20 … it physically cannot exceed
them" holds for *transfers* and not for *redemption*.

The behaviour is deliberate for the case it was written for — a merchant who received
wrapped units cashing them out, which is what makes accepting Leash "as good as cash."
The gap is that nothing distinguishes a merchant's *received* units from a capability's
*unspent* ones. Both are just wrapped-token balances.

**The fix, as landed.** None of the three directions originally sketched here survived
contact with the code. Two of them ("reject when the source *is* a capability's token
account") would have broken the legitimate unwind path: `week2_issue_redeem.rs` redeems
from exactly that account, as the root principal, and blocking it would mean the person
who funded the vault could never get their deposit back. The third (route redemption
through the hook) fails differently — the hook resolves a capability from the source's
owner, and a merchant has none, so merchants could no longer redeem at all.

The distinction that actually works is **who funded the vault**:

| Holder | May redeem? |
|---|---|
| Merchant (no capability at `[CAPABILITY_SEED, holder]`) | Yes, freely — the units arrived through a real, hook-checked transfer |
| Root capability owner (`depth == 0`) | Yes, but only `cap - spent - committed_to_children`, and `cap` shrinks by the amount redeemed |
| Delegated capability owner (`depth > 0`) | **No.** It never deposited. It may still spend through the hook to an allowlisted destination |

`redeem` now takes the holder's Capability PDA, address-verified by a `seeds`
constraint. It is passed unconditionally rather than as an `Option`: a caller able to
omit it could opt out of the check entirely. A holder with no capability passes a PDA
that does not exist, and the handler verifies non-existence on-chain (program-owned and
non-empty) instead of trusting the caller.

The root bound matters as much as the child ban: without the `committed_to_children`
term a parent could drain the vault and strand the units minted for its children — the
same collateral shortfall as 0.2, reached through redemption instead of spending.
Shrinking `cap` in step keeps the capability from advertising spending power the vault
no longer backs, and is verified: after redeeming its free budget the root's next spend
is rejected.

Covered by `programs/leash-program/tests/redeem_authorization.rs` — the delegated
cash-out attempt (also when revoked), the merchant path still working, and the root
boundary with its `cap` write-back. **Verified to fail without the fix**: with the
authorization block disabled and the program rebuilt, the delegated agent's redemption
succeeds and the root drains past its committed budget.

Two things this deliberately does not do: reclaiming a finished child's reservation back
to its parent needs a new instruction (nothing releases `committed_to_children` today),
and the derivation assumes one capability per owner, so it must be revisited when 0.3
lands — at that point it should derive from `holder_wrapped_account`, making the
association exact rather than "the capability this holder happens to have."

### 0.2 `committed_to_children` was never enforced on the spend path — **critical**

- [x] **Fixed.** A parent could spend budget it had already delegated, so total spend
      exceeded the deposit backing it. Found by reading the code; not inherited from the
      build plan.

`attenuate` reserves budget correctly. It computes
`parent_remaining = cap - spent - committed_to_children` and rejects a child whose cap
exceeds it (`attenuate.rs:81-86`), then increments the parent's `committed_to_children`
(`attenuate.rs:162-167`).

Nothing honours that reservation. Both enforcement points check only
`spent + amount <= cap`:

- `leash-hook/src/lib.rs:214-217` — the hook's validation.
- `record_spend.rs:51-55` — the authoritative writer, whose re-check is documented as
  deliberate defense-in-depth and makes the identical omission.

Neither subtracts `committed_to_children`.

The reason this converts into missing money is that attenuation **mints fresh units**
rather than moving the parent's (`attenuate.rs:115-126`; the code comments the intent
explicitly at `:160-161` — "accounting only, no token movement on the parent's side").
So the parent's token account still holds its full original balance after delegating.

Concretely:

1. Principal issues `cap = 100`, depositing $100 → vault $100, wrapped supply $100.
2. Principal attenuates $20 to an agent → $20 fresh units minted. Supply is now $120
   against a $100 vault. Parent still holds $100; `committed_to_children = 20`.
3. Parent spends $100 to an allowlisted destination. Hook: `0 + 100 <= 100` → **passes**.
   Parent is now `spent=100, committed=20, cap=100` → `120 > 100`.
4. Agent spends its $20. Hook: `0 + 20 <= 20` → **passes**.
5. $120 has been spent against a $100 deposit. The last $20 of merchant redemptions hits
   an empty vault.

No unusual behaviour is required at any step — this is the ordinary delegate-then-spend
flow, through the hook that exists to prevent exactly this.

What makes it worth flagging loudly rather than filing quietly: the invariant is
*promised in writing, at the place a reader would check*. `state.rs:21-22`, on the
`Capability` struct itself:

```
/// Invariant (checked by the program, not by convention — see BUILD_PLAN.md §2/§3):
///     spent + committed_to_children <= cap
```

`BUILD_PLAN.md:82` repeats it: "Invariant checked on every mutation:
`spent + committed_to_children <= cap`." It is not checked on any mutation.

**The fix, as landed:** both enforcement points now check
`spent + committed_to_children + amount <= cap` — `leash-hook`'s `spend_logic` and
`record_spend`. Fixing only the hook would have been insufficient: `record_spend` is what
actually writes `spent`, so it has to hold independently of whether the hook's arithmetic
is right. (That second check is unreachable from outside — nothing but the hook can
produce the required `hook-authority` signature — so it is defense in depth that no test
can exercise directly, which is exactly why it is written to stand alone.)

Covered by `programs/leash-program/tests/conservation_invariant.rs`: the full
delegate-then-overspend sequence, the exact 600/601 boundary, and accumulation across
multiple children. All three assert `LeashHookError::CapExceeded` (6004) specifically via
the new `expect_err_code`, not `is_err()` — which matters here, because the pre-fix
behaviour and a Token-2022 insufficient-funds rejection are indistinguishable to a bare
failure check. **Verified to fail without the fix**: with the two checks reverted and the
programs rebuilt, all three fail with "expected … to fail with Custom(6004), but it
succeeded."

**Interaction with 0.1:** the two are independent and were fixed separately. 0.2 bounds
what a capability tree can *spend*; it says nothing about redemption, which does not go
through the hook at all. 0.1's fix carries the same `committed_to_children` term into
the redeem path for exactly this reason — otherwise the shortfall 0.2 closes would still
be reachable by cashing out instead of spending.

### 0.3 One owner can only ever hold one capability

- [~] **Designed, not landed.** Root capabilities are derived at `[CAPABILITY_SEED,
      principal]` (`issue.rs:46`, which still carries its literal `// TODO: real seed
      scheme (nonce for multiple root caps per principal)`) and children identically at
      `[CAPABILITY_SEED, child_owner]` (`attenuate.rs:42`). With no nonce, one owner
      pubkey can hold at most one capability, ever — a second `issue` or `attenuate` for
      the same owner collides with the first PDA and fails inside Anchor's `init` with an
      opaque account-already-exists error. A silent trap for the first real user who
      mints twice.

This is not a one-line fix, and the reason is worth preserving. `leash-hook` re-derives
"the Capability for this transfer" from a **single** seed formula, registered once into
the wrapped mint's `ExtraAccountMetaList` at deployment (`leash-hook/src/lib.rs:59-69`,
seeded on base account 3 — the Transfer Hook Interface's "owner" slot). It resolves only
from accounts already present in a transfer and cannot be handed a client-chosen nonce
out-of-band. Two different derivation schemes for root vs. child capabilities cannot both
be that one formula — which is exactly why `attenuate.rs:29-37` documents
one-capability-per-owner as a deliberate consequence rather than an oversight.

Intended approach: give every capability its own dedicated wrapped-token account seeded
`[TOKEN_ACCOUNT_SEED, owner, nonce]`, and key the Capability PDA off *that account's
address*. The hook's formula is then unchanged in mechanism — it points at base account 0
(the transfer's source token account) instead of base account 3, because the nonce is
already folded into that address and the interface supplies it on every transfer. The SDK
generates a random `u64` nonce unless one is passed; the CLI exposes `--nonce`. Because
an owner is no longer a unique key, `spend`/`revoke`/`watch`/`attenuate` must take an
explicit `--capability`.

**Why this is `[~]` and not `[x]`:** it was implemented on a branch called
`fix/capability-seed-nonce` inside an ephemeral cloud sandbox that had no git remote
configured, so it was never pushed. That sandbox also had no `anchor` or
`cargo-build-sbf` and no network access to the toolchain, so **it never produced a `.so`
and never ran a single test** — including the tests written to prove the fix. Its IDL was
hand-patched rather than regenerated. The branch exists nowhere now: `main` is the only
branch on `origin`. Verifiably absent from `main`: `TOKEN_ACCOUNT_SEED`,
`multi_capability`, `expect_err_code`.

**This is a breaking on-chain change.** Instruction arguments and account lists both
change, so the existing devnet deployment and any capabilities issued under the old
scheme are incompatible. Redeploy, don't upgrade-in-place over live state.

### 0.4 The capability ↔ token-account binding is nominal

- [ ] **`Capability.token_account` is written and never read.** Found by reading the
      code; not inherited from the build plan.

The field is set at `issue.rs:131` and `attenuate.rs:150`, declared in `state.rs`, and
sized into `Capability::MAX_SIZE` — and read by nothing. Not the hook, not
`record_spend`, not `redeem`. `spend_logic` never inspects `accounts[0]`, the transfer's
source token account; it resolves the capability purely from the owner (see 0.3).

So the field documents an association the program never enforces. Any wrapped balance an
owner holds debits that owner's capability, whether or not it is the balance the
capability was issued against.

Largely subsumed by 0.3 — once the Capability PDA is keyed off the token account's
address, the binding becomes structural rather than advisory. But 0.3 alone does not
finish it: the hook must **additionally** check `source == capability.token_account`.
Without that check, the derivation proves the account is *a* capability's token account,
not that it is *this* transfer's.

### 0.5 Cap enforcement isn't isolated from balance enforcement — and the tests can't tell either

- [ ] A spend over the cap is currently indistinguishable from Token-2022's own
      insufficient-balance rejection, because a capability's token balance always equals
      its remaining cap — `issue` mints exactly `cap`, once, and nothing ever adds to it.
      Documented honestly in `week3_spend_enforcement.rs`, `tests/invariants/README.md`,
      and `BUILD_PLAN.md:340`; the Week 6 devnet run shows the same ambiguity in the wild
      as `custom program error: 0x1` (`BUILD_PLAN.md:481`).

      So the existing over-cap test proves "spending more than was issued fails," not
      specifically "leash-hook's `amount + spent > cap` check works."

- [~] **The test suite could not distinguish them even if the balances differed.**
      `expect_err` asserts `is_err()` and then *prints* the error to stderr — it never
      inspects it. `expect_err_code` now exists alongside it (`tests/common/mod.rs`) and
      is used throughout `conservation_invariant.rs`, but the three original test files
      still use the weaker helper, so their rejections remain unverified as to cause.
      Converting them is the remaining work here.

      This means **CLAUDE.md and BUILD_PLAN.md overstate the committed tests** where they
      claim every rejection is verified by its actual on-chain error code. That practice
      did happen during development — it is how the Week 3 `AlreadyProcessed`
      false-positives were caught — but it was never encoded as an assertion. Both
      documents should be corrected.

      `expect_err_code` asserts the specific `LeashError` / `LeashHookError` variant
      (hook variants: `Revoked` 6000, `ParentRevoked` 6001, `Expired` 6002,
      `NotAllowlisted` 6003, `CapExceeded` 6004). It was added as a prerequisite for
      0.2's test, since without it that test would have passed for the wrong reason.

      Fully isolating cap-from-balance additionally needs a scenario where balance exceeds
      cap, which no current instruction produces. Note that 0.2's over-issuance was *not*
      a usable source of one — it was a bug to fix, not a fixture to build on, and it is
      now fixed.

### 0.6 No way to enumerate an owner's capabilities

- [ ] Once one owner can hold many (0.3), there is no on-chain index. A caller who loses
      a nonce cannot find the capability again except by scanning program accounts
      filtered on the `owner` field.

      Cheap fix: a `getProgramAccounts` helper in the SDK with a `memcmp` filter at the
      `owner` offset (8, right after the discriminator — the layout is already fixed and
      documented in `state.rs`). Expensive fix: a per-owner registry account, which
      reintroduces a hot account and a write on every issue. Start with the former.

      Blocked on 0.3; meaningless before it.

### 0.7 Budget reserved for a child is never released back

- [x] **Reconciliation on redeem: fixed as part of 0.1.** Redeeming used to leave `cap`
      and `spent` untouched, so a capability's recorded budget could outlive the tokens
      backing it. A root's redemption now decrements `cap` by the amount taken out, which
      is asserted directly (`root_redemption_is_bounded_by_committed_budget`) — including
      that a root which has redeemed its free budget can no longer spend. The other two
      redeem paths need no reconciliation: a merchant has no capability, and a delegated
      capability cannot redeem at all.

- [ ] **Still open: nothing releases `committed_to_children`.** When a child is finished
      with — revoked, expired, or simply done — the budget its parent reserved stays
      reserved forever. The parent cannot spend it (0.2 enforces the reservation) and
      cannot redeem it (0.1 bounds redemption by the same term), so the collateral is
      stranded, correctly but permanently.

      This is the cost of both critical fixes being conservative: they close the holes by
      refusing, and nothing yet un-refuses. Needs a `release`/`reclaim` instruction that
      burns a revoked child's unspent units and decrements the parent's
      `committed_to_children` by the same amount — the only way to do it without
      reopening 0.2, since the two numbers have to move together.

### 0.8 Allowlist ergonomics and size

- [ ] Allowlist entries are **destination token accounts**, not merchant identities — the
      hook compares `destination.key` directly against the `Vec<Pubkey>`. That means a
      merchant rotating or adding a token account silently falls off every existing
      allowlist, and callers must know the token account rather than the merchant's
      wallet. Worth deciding deliberately rather than inheriting.
- [ ] `MAX_ALLOWLIST_LEN` is 10, stored as a flat `Vec<Pubkey>` inside a fixed-size
      account. Merkle-proof allowlists are the known answer (BUILD_PLAN.md §12) and are
      not needed until 10 is the binding constraint — but 10 is low enough that it may
      bind sooner than expected.

---

## Phase 1 — Assurance

Nothing here changes behaviour. It's the work that makes the claim "this is correct"
survive someone else's scrutiny — which, per leash.txt's own risk list, is the difference
between a credible primitive and an unrecoverable incident.

- [ ] **Randomized fuzzing.** The "fuzz suite" in `tests/invariants/README.md` is a
      checklist of specific, hand-built cases — the README says so explicitly. Genuine
      property-based testing over *sequences* of instructions (`trident`, or `proptest`
      over a state machine) is a different thing and is a real gap.

      The invariant to drive it is `spent + committed_to_children <= cap`, for every node,
      after every operation. That invariant now holds on `main` (0.2), and is enforced at
      both write paths — but it is currently proven only by the hand-built cases in
      `conservation_invariant.rs`. Fuzzing is what would have found 0.2 in the first
      place; it is what would show whether a sibling case survives.
- [ ] **A written argument for the conservation invariant.** `checked_*` arithmetic is
      used throughout, but the invariant currently holds — where it holds at all — by
      case-by-case inspection of `issue`/`attenuate`/`record_spend`. It deserves an
      explicit proof sketch naming every write path to `cap`, `spent`, and
      `committed_to_children`. 0.2 is what inspection-without-a-written-argument missed.
- [ ] **External review.** Non-negotiable before mainnet, and BUILD_PLAN.md §10 already
      says so: "ideally, someone other than the author has read the program." An audit is
      the strong form; a second engineer reading the two programs is the minimum. That
      0.1, 0.2, and 0.4 were all found by a first careful outside read is the argument for
      doing this properly.
- [ ] **A security self-review checklist**, written up with findings and fixes, hunting
      the bug classes that actually break Solana programs: missing signer/owner checks,
      integer overflow, PDA seed collisions, CPI privilege escalation, and — added by
      experience here — invariants asserted in comments but never in code.
- [ ] **Hook-failure semantics under adversarial conditions.** Property 6 says spends
      fail *closed* if the enforcing program is unreachable. That's argued from the
      Token-2022 hook mechanism but not tested — e.g. a malformed or truncated
      `ExtraAccountMetaList`, or a capability account that fails to deserialize, should
      reject the transfer rather than skip enforcement.
- [ ] **Upgrade-authority policy.** Both programs are deployed to devnet upgradeable,
      under the same authority as the committed program keypairs (`BUILD_PLAN.md:462`).
      Who holds that authority on mainnet, and whether it's eventually burned or moved to
      a multisig, is a trust question that has to be answered before anyone deposits real
      funds — it currently isn't documented anywhere.
- [ ] **There is no CI.** No `.github/workflows` directory exists at all. Every check in
      this document that says "add a CI step" is new infrastructure, not a repair to
      something already running.

## Phase 2 — Ergonomics and developer experience

- [ ] **Publish `@leash/sdk` and `@leash/cli` to npm.** Both sit at `0.1.0`, unpublished.
      Right now "using Leash" means cloning this repo and building it locally — a real
      barrier for anyone not already deep in it.
- [ ] **A real, minimal integration example** — one working reference (e.g. a small
      service accepting a Leash-capped payment end-to-end) alongside the API docs, so a
      new user has something to copy from, not just a command reference.
- [ ] **Remaining budget isn't directly queryable.** Callers compute
      `cap - spent - committed_to_children` themselves. That expression appearing in
      three places (program, SDK, any consumer) is how definitions drift apart; it belongs
      on `CapabilitySnapshot` as a derived field. 0.2 is what that drift looks like when
      one of the three copies is missing.
- [ ] **Deployment config.** A single `leash-deployment.json` in the working directory,
      per `cli/src/config.ts`. Fine for a demo, wrong for anyone with more than one
      deployment or a CI environment. Wants named profiles and an env-var override.
- [ ] **Rust/TypeScript seed parity is maintained by hand.** `sdk/ts/src/constants.ts`
      documents this and warns about it. Either generate the TS constants from the Rust
      source or add a parity test that fails when they diverge.
- [ ] **The committed IDL can drift from the programs.** `sdk/ts/src/idl/*` are generated
      artifacts checked into the repo (deliberately — so consuming the SDK doesn't require
      an Anchor toolchain). Nothing enforces that they were regenerated after a program
      change. A CI step that runs `npm run generate-idl` and fails on a dirty tree closes
      this.
- [ ] **Python SDK.** Named in leash.txt's MVP as shipping alongside the TS one; most
      agent frameworks are Python. Not started.

## Phase 3 — The acceptance side

The one-sided version of Leash — bounding your *own* agent's spend — works today. The
two-sided version, where services natively accept capabilities, is where leash.txt argues
the actual value is, and it explicitly names cold-starting acceptance as a top risk.

- [ ] `leash-verify` — small middleware (Express/FastAPI/Hono) letting any API accept and
      verify a capability, per BUILD_PLAN.md §12 and leash.txt §7.
- [ ] Framework adapters and an MCP server.
- [ ] x402 client integration.

Deliberately after Phase 0 and 1. Distribution work on top of an unproven primitive is
the trap leash.txt's self-critique names directly.

---

## Later phases — unchanged from BUILD_PLAN.md §12

Carried forward as-is, still deliberately unspecified. Each is a real step and a real
distraction if taken early:

- **Sharded budgets + mint-side voucher merge** — once one parent spent against by many
  concurrent children serializes on a single hot account. Don't design it before you can
  measure the contention.
- **ZK / state compression** — makes thousands of short-lived per-task capabilities cheap.
  An economic optimization, not a requirement; leash.txt's own self-critique is explicit
  that overstating this weakens the Solana argument.
- **Merkle-proof allowlists** — see 0.8; needed once a flat `Vec<Pubkey>` stops being
  cheap.
- **Velocity / rate limits** — a real feature, deferred because cap + expiry + allowlist
  already carry the core guarantee.
- **Confidential transfers** — named in leash.txt as what makes the product adoptable at a
  company, since an org's agent spend pattern is its operational roadmap. Not needed for
  the indie wedge.

## Explicitly still not on this roadmap

A dashboard, a hosted service, a wallet, a token, an enterprise console, a marketplace.
leash.txt §13 predicts that within four months someone will offer money for a hosted
console, and that building it converts the project from a primitive into "a SaaS company
with a blockchain inside." Listed here so that "not on the roadmap" reads as a decision
rather than an oversight.

Also not here: sharded budgets, ZK/state compression, merkle-proof allowlists, velocity
limits, confidential transfers, secp256r1/passkey signing, framework adapters, x402
integration — all real, all later, per BUILD_PLAN.md §9 and §12.

## What "ready for mainnet" would mean

Concretely, so it's falsifiable rather than a vibe:

1. Every item in Phase 0 closed, each with a test that fails without the fix — and, per
   0.5, each asserting the specific on-chain error code rather than just failure.
2. Phase 1's fuzzing and external review done.
3. A conservative per-capability cap enforced on-chain (leash.txt suggests ~$500) so that
   a bug that survives all of the above is survivable.
4. Upgrade authority policy documented and executed.
5. `@leash/sdk` and `@leash/cli` installable from npm by someone outside this repo, and
   one integration example working end-to-end.
6. A real mainnet capability issued, spent against, and revoked under that cap ceiling,
   with transaction links to prove it — not a claim, a link.

Until then: devnet. BUILD_PLAN.md §10 names real-money temptation as a specific risk of
this project, and 0.1 and 0.2 above are concrete demonstrations that the instinct was
right — two fund-losing bugs in a codebase whose tests were all green. Both are fixed;
what should carry forward is that a green suite said nothing about either one.

## Verification approach

Same standard as the rest of this repo: real transactions against real programs, every
rejection checked against its actual on-chain error code, and an explicit note of what's
honestly not verified yet rather than a claim that something "looks done."

That standard is currently aspirational in one respect — see 0.5, where the committed
tests assert only that a call failed. Closing 0.5 is what makes this paragraph true of
the code as well as of the intent.
