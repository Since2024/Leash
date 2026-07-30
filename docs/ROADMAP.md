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

0.3 and 0.4 are now fixed too, together: capabilities are keyed on their own token
account rather than their owner, so one owner can hold many, and the hook checks the
binding it previously only recorded. Both are covered by `multi_capability.rs`, verified
to fail without the fix, and proven end-to-end against a real validator. Two consequences
worth carrying forward: the **devnet deployment is now stale** — the instruction ABI
changed and it has not been redeployed — and 0.6 (no way to enumerate an owner's
capabilities) went from theoretical to live, and is now fixed too, via a
`getProgramAccounts` helper and a `leash list` command. 0.8 is new, surfaced by 0.3: a
parent cannot revoke one delegation without cutting off all of them.

**Phase 0 now stands at 0.1 through 0.8 done.** 0.7 and 0.8 landed together as
`reclaim` + `revoke_descendant`, which is what finally makes "cut this agent off and take
the money back" expressible — the loop the pitch in `leash.txt` describes and the code
could not previously perform. Remaining: 0.9 (allowlist ergonomics, a product decision
more than a correctness gap) and 0.10, which 0.7 introduced knowingly — dead children's
units can't be burned, so wrapped supply overstates redeemable value even though the vault
covers everything that can actually move.

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

- [x] **Fixed.** Root capabilities were derived at `[CAPABILITY_SEED, principal]`
      (`issue.rs:46`, which carried a literal `// TODO: real seed scheme (nonce for
      multiple root caps per principal)`) and children identically at `[CAPABILITY_SEED,
      child_owner]` (`attenuate.rs:42`). With no nonce, one owner pubkey could hold at
      most one capability, ever — a second `issue` or `attenuate` for the same owner
      collided with the first PDA and failed inside Anchor's `init` with an opaque
      account-already-exists error. A silent trap for the first real user who minted
      twice.

This is not a one-line fix, and the reason is worth preserving. `leash-hook` re-derives
"the Capability for this transfer" from a **single** seed formula, registered once into
the wrapped mint's `ExtraAccountMetaList` at deployment (`leash-hook/src/lib.rs:59-69`,
seeded on base account 3 — the Transfer Hook Interface's "owner" slot). It resolves only
from accounts already present in a transfer and cannot be handed a client-chosen nonce
out-of-band. Two different derivation schemes for root vs. child capabilities cannot both
be that one formula — which is exactly why `attenuate.rs:29-37` documents
one-capability-per-owner as a deliberate consequence rather than an oversight.

**The fix, as landed.** Every capability gets its own dedicated wrapped-token account
seeded `[TOKEN_ACCOUNT_SEED, owner, nonce]`, and the Capability PDA is keyed off *that
account's address*. The hook's formula is unchanged in mechanism — it points at base
account 0 (the transfer's source token account) instead of base account 3, because the
nonce is already folded into that address and the interface supplies it on every
transfer. `issue`/`attenuate` take a `nonce: u64`; the SDK generates a random one unless
passed and returns the one it used; the CLI exposes `--nonce` and prints it. Because an
owner is no longer a unique key, `spend`/`revoke`/`watch`/`attenuate` now require an
explicit `--capability` or `--nonce` and **refuse to guess** — defaulting to, say, nonce
0 would silently act on the wrong capability for anyone holding several, which is the
failure this change exists to prevent.

The capability's token account is now created by Anchor's `init` + `token::*`
constraints, replacing the hand-rolled CPI to the associated-token-account program in
both `issue` and `attenuate`. It is program-derived, but `token::authority` remains the
holder, so the bearer-object model of BUILD_PLAN.md §0 is intact: only the *address* is
derived, not control.

Covered by `programs/leash-program/tests/multi_capability.rs` (7 tests): two roots for
one principal with independent budgets, spending one not debiting the other, independent
caps, independent revocation, one parent delegating to the same agent twice, and units
held outside a capability's own account not counting as its budget. **Verified to fail
without the fix**: with the nonce dropped from the token-account seeds and the program
rebuilt, the second `issue` dies with `Allocate: account ... already in use` — exactly
the collision described above.

Also **proven end-to-end against a real `solana-test-validator`** through the CLI, the
same bar Week 5 set: `init` → two `mint`s for one owner → `spend` from the first →
on-chain state showing the first charged 200 and the second untouched → `revoke` the
first → its next `spend` fails with `Error Code: Revoked` thrown from inside leash-hook
during Token-2022's own `TransferChecked`, while the second capability spends its full
budget successfully in the same session.

One toolchain trap worth recording: **`anchor build`'s `.so` does not run on this local
validator** — it fails at 44 compute units with `Access violation in unknown section at
address 0x4`, before any program logic. `anchor build` is still the right way to
regenerate the IDL (hand-patching it is what made the previous attempt untrustworthy),
but the binaries must come from `cargo-build-sbf`, and the validator must be restarted
against those. Diagnosed by rebuilding and restarting, not guessed.

**This was a breaking on-chain change.** Instruction arguments and account lists both
changed, so the existing devnet deployment and any capabilities issued under the old
scheme are incompatible. Redeploy, don't upgrade-in-place over live state — the devnet
programs listed in CLAUDE.md predate this and have **not** been redeployed yet.

### 0.4 The capability ↔ token-account binding is nominal

- [x] **Fixed, alongside 0.3.** `Capability.token_account` was written and never read.
      Found by reading the code; not inherited from the build plan.

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

**As landed:** that check is now in `leash-hook`'s `spend_logic`
(`LeashHookError::WrongTokenAccount`), and the equivalent one in `redeem`
(`LeashError::Unauthorized`). Both are belt-and-braces rather than load-bearing —
address derivation already ties the capability to the source account — so they guard
against the field being written wrong at issue time, not against a forged account. They
are cheap, and a field that is written twice and read by nothing is exactly the sort of
thing that quietly stops being true. `multi_capability.rs` asserts the field matches the
account each capability was actually issued against, and covers the case where an owner
holds capability budget *and* separately-received units: the received units are not
spendable as capability budget.

### 0.5 Cap enforcement isn't isolated from balance enforcement — and the tests can't tell either

- [x] **Fixed.** A spend over the cap used to be indistinguishable from Token-2022's own
      insufficient-balance rejection, because a capability's token balance always equals
      its remaining cap — `issue` mints exactly `cap`, once, and nothing ever adds to it.
      Documented honestly in `week3_spend_enforcement.rs`, `tests/invariants/README.md`,
      and `BUILD_PLAN.md:340`; the Week 6 devnet run shows the same ambiguity in the wild
      as `custom program error: 0x1` (`BUILD_PLAN.md:481`).

      So the over-cap test proved "spending more than was issued fails," not specifically
      "leash-hook's `amount + spent > cap` check works."

      **The premise that made this look hard was wrong** — see the correction under the
      second bullet. Isolating the two needs a capability whose *balance exceeds its
      spendable budget*, and `attenuate` produces exactly that: it mints the child's units
      fresh rather than moving the parent's, so a parent that delegates 200 of its 500
      still holds all 500 while only 300 remains spendable. 301 is then comfortably inside
      the token balance — Token-2022 cannot be the one rejecting — so a `CapExceeded` there
      can only have come from leash-hook's own arithmetic.

      `week3_spend_enforcement.rs` now asserts exactly that, on the `principal3` chain it
      was already building for the ancestor test. The old ambiguous 901 case is kept but
      relabelled and asserted as `E_TOKEN_INSUFFICIENT_FUNDS` (Token-2022's own error 1),
      so the suite now names *which layer* rejected instead of blurring them together.

- [x] **Fixed.** `expect_err` asserts `is_err()` and then *prints* the error to stderr —
      it never inspects it. `expect_err_code` was added alongside it
      (`tests/common/mod.rs`) as a prerequisite for 0.2's test, since without it that test
      would have passed for the wrong reason, but the three original `week*` files kept
      using the weaker helper, so their rejections were unverified as to cause.

      All ten of those are now converted: `NotAllowlisted` (6003), `Revoked` (6000),
      `ParentRevoked` (6001) for each of the three ancestor depths, `Expired` (6002),
      `CapExceeded` (6000, leash-program's own) for over-delegation, `DepthExceeded`
      (6004) past `MAX_DEPTH`, and Token-2022's `InsufficientFunds` (1) for the balance
      case.

      The conversion was worth doing for more than tidiness: all three ancestor tests turn
      out to throw `ParentRevoked`, *not* `Revoked`, so the hook really does distinguish
      "this capability is revoked" from "an ancestor is" — a distinction the old
      `is_err()` assertions could not have caught either way. **The new assertions were
      themselves verified to discriminate**: swapping one `ParentRevoked` for `Revoked`
      makes the test fail with `expected ... Custom(6000), but got: Custom(6001)`, so they
      are really reading the code and not merely matching any error.

      Two `expect_err` calls remain, both in `multi_capability.rs` and both deliberate,
      with the reason stated at the call site: one is a genuinely balance-bound rejection
      (the isolated version of the same check lives in the test above it), and the other
      fails during client-side extra-account resolution, where there is no on-chain error
      code to assert at all.

      This also retires a documented overstatement: **CLAUDE.md and BUILD_PLAN.md used to
      claim every rejection was verified by its on-chain error code** when it was not
      (corrected in commit f3f4eb9). That claim is now true of the committed tests.

      ~~Fully isolating cap-from-balance additionally needs a scenario where balance
      exceeds cap, which no current instruction produces.~~ **Wrong, and corrected above:**
      `attenuate` produces exactly that scenario, because it mints the child's units
      instead of moving the parent's. `conservation_invariant.rs` had been relying on this
      since 0.2 without the connection being drawn back here. (The rest of the original
      note stands: 0.2's over-issuance was a bug to fix, not a fixture to build on.)

### 0.6 No way to enumerate an owner's capabilities

- [x] **Fixed (the cheap way, as planned).** Once one owner can hold many (0.3), there
      was no on-chain index, and a caller who lost a nonce could not find the capability
      again except by scanning program accounts filtered on the `owner` field.

      Landed as the cheap fix this entry called for: `findCapabilitiesByOwner` in
      `sdk/ts/src/find.ts` uses `getProgramAccounts` with a `memcmp` filter at the `owner`
      offset (8, right after the discriminator — verified against `state.rs`, not
      assumed), plus a `leash list` CLI command. The expensive fix — a per-owner registry
      account — stays unbuilt, since it reintroduces a hot account and a write on every
      `issue`.

      **What it deliberately does not do: recover a random nonce.** The nonce is hashed
      into the token account's address, so it cannot be read back out; it is only
      recoverable by deriving candidates and comparing. `--recover-nonce <limit>` does
      exactly that and therefore only ever finds small sequential nonces — the `--nonce 0`,
      `--nonce 1` pattern a human types — and essentially never one from the SDK's
      `randomNonce()`. Both the SDK and the CLI say so rather than printing a blank
      column: an unrecovered nonce prints `nonce=? (not in scanned range)`. Capabilities
      themselves are always listed in full regardless, so nothing is hidden — only the
      convenience of re-deriving addresses is lost.

      So the durable answer is still to keep the nonce `mint`/`attenuate` return. This
      makes a lost one survivable, not a non-issue.

      Also worth knowing before depending on it: `getProgramAccounts` is a heavy RPC call
      that scans every account the program owns, and many public endpoints rate-limit or
      disable it. Fine for a CLI or a dashboard refresh; not for a hot path.

      Verified against a real `solana-test-validator`: three capabilities for one owner
      (nonces 0, 1, and a random one) were all found by scan; `--recover-nonce 16`
      recovered 0 and 1 and correctly reported the random one as out of range; and a
      nonce recovered from `list` was then used to `spend`, with the result showing up in
      the next `list`.

### 0.7 Budget reserved for a child is never released back

- [x] **Reconciliation on redeem: fixed as part of 0.1.** Redeeming used to leave `cap`
      and `spent` untouched, so a capability's recorded budget could outlive the tokens
      backing it. A root's redemption now decrements `cap` by the amount taken out, which
      is asserted directly (`root_redemption_is_bounded_by_committed_budget`) — including
      that a root which has redeemed its free budget can no longer spend. The other two
      redeem paths need no reconciliation: a merchant has no capability, and a delegated
      capability cannot redeem at all.

- [x] **Fixed: `reclaim` releases `committed_to_children`.** When a child was finished
      with — revoked, expired, or simply done — the budget its parent reserved stayed
      reserved forever. The parent could not spend it (0.2 enforces the reservation) and
      could not redeem it (0.1 bounds redemption by the same term), so the collateral was
      stranded, correctly but permanently. That was the cost of both critical fixes being
      conservative: they close the holes by refusing, and nothing un-refused.

      **The specified fix was not implementable, and the reason is worth keeping.** This
      entry called for an instruction that "burns a revoked child's unspent units and
      decrements the parent's `committed_to_children` by the same amount." The burn cannot
      be done: a capability's token account is `token::authority = <holder>` — the
      bearer-object model of BUILD_PLAN.md §0 — so burning from it requires the *child's*
      signature, and a child being reclaimed from has just been revoked by its parent.
      Requiring its cooperation would make the instruction useless in exactly the case it
      exists for, and making the program the authority would break the bearer model.

      So `reclaim` (`instructions/reclaim.rs`) moves **accounting only**, and rests on the
      child being provably unable to spend: `revoked` is one-way, `expiry` is fixed at
      `attenuate` time and cannot be extended, leash-hook checks both on every spend, and
      a delegated capability cannot `redeem` at all (0.1). A dead child's units are inert
      by three independent paths, so releasing the reservation cannot let the tree spend
      more than the vault backs — the invariant 0.2 protects. It releases `cap - spent`,
      never the full delegation, since units the child already spent are genuinely gone.

      Guards, each covered by a test: refuses while the child is live
      (`ChildStillLive`); only the **immediate** parent may call it (`NotAChild`), because
      `attenuate` records the reservation nowhere else; and it is idempotent, writing the
      child's `cap` down to `spent` so a second call releases nothing rather than crediting
      the parent twice. **The liveness guard was verified to be load-bearing**: disabling
      it and rebuilding makes two tests fail.

      Covered by `programs/leash-program/tests/reclaim_and_descendant_revoke.rs`, and
      proven end-to-end on a real validator through the CLI: delegate 400, agent spends
      150, `revoke-descendant`, `reclaim`, and the parent's spendable budget goes from 400
      to 850 — with 851 rejected by the hook and the merchant's final balance exactly the
      1_000 deposited.

      **Known artifact, tracked as 0.10:** the dead child's unspent units are never
      destroyed, so the wrapped mint's total supply overstates redeemable value.

### 0.8 A parent cannot revoke a single delegation

- [x] **Fixed: `revoke_descendant`.** `revoke` is `has_one = owner` (`revoke.rs:15`), so
      only a capability's *own* owner could flip its `revoked` flag. A parent's only lever
      over a child was revoking **itself**, which cascades to every descendant through the
      hook's ancestor walk. Found while writing `multi_capability.rs`, whose first draft
      assumed the parent could revoke one child and was rejected on-chain with
      `Unauthorized` (6006); that test pins the old rule's rejection, and the new
      instruction is what makes selective revocation possible.

      Authority comes from the target's own `ancestors` array — the same array leash-hook
      already reads on every spend — so this adds no new state and no new trust: if a
      capability's spends are already gated on these ancestors being unrevoked, those same
      ancestors are exactly the set entitled to revoke it. **Any** ancestor may, not just
      the immediate parent, because a grandparent could already stop those spends by
      revoking itself; letting it revoke the descendant directly is strictly less
      destructive than the lever it already had.

      Kept as a separate instruction rather than a branch inside `revoke`: the authority
      check is genuinely different (`has_one` versus an ancestry proof), and the common
      self-revoke path shouldn't pay for a check it never needs.

      Note the split from 0.7, which is deliberate and tested: any ancestor may *revoke*,
      but only the immediate parent may *reclaim*, since the reservation exists only
      there.

      Why it mattered: this was invisible while an owner could hold only one capability.
      Once a parent can delegate to the same agent several times (0.3), "withdraw *this*
      allowance and leave the others alone" is a natural thing to want, and it was not
      expressible — the parent had to cut off everything downstream or nothing. It also
      read oddly against the pitch in `leash.txt`, where revocation is sold as the
      principal's power to cut an agent off, while in fact the agent could revoke its own
      capability and the principal could not revoke just that one.

      Landed together with 0.7, as this entry predicted they should be: revoking a child
      is exactly when its reserved budget should become reclaimable.

### 0.9 Allowlist ergonomics and size

- [ ] Allowlist entries are **destination token accounts**, not merchant identities — the
      hook compares `destination.key` directly against the `Vec<Pubkey>`. That means a
      merchant rotating or adding a token account silently falls off every existing
      allowlist, and callers must know the token account rather than the merchant's
      wallet. Worth deciding deliberately rather than inheriting.
- [ ] `MAX_ALLOWLIST_LEN` is 10, stored as a flat `Vec<Pubkey>` inside a fixed-size
      account. Merkle-proof allowlists are the known answer (BUILD_PLAN.md §12) and are
      not needed until 10 is the binding constraint — but 10 is low enough that it may
      bind sooner than expected.

### 0.10 Wrapped supply overstates redeemable value

- [ ] **Introduced deliberately by 0.7**, and the reason it is acceptable is written up
      there: `reclaim` cannot burn a dead child's unspent units, because the token
      account's authority is the child (the bearer model), not this program. Those units
      are inert — a revoked or expired capability cannot spend, and a delegated one cannot
      redeem — but they still count toward the wrapped mint's `supply`.

      Nothing on-chain reads total supply, so nothing currently breaks. The risk is
      **interpretive**: anyone auditing solvency by comparing wrapped supply against the
      vault balance will see a shortfall that is not real, and the gap grows with every
      delegation that ends without being fully spent. That is a bad property for a system
      whose whole pitch is that the token enforces the limit.

      Note this is distinct from 0.2's over-issuance, which was a genuine solvency hole.
      Here the vault is fully sufficient for every unit that can actually move; only the
      headline number is wrong.

      Options, none obviously right yet: have `attenuate` make the program a delegate on
      the child's token account at creation time so `reclaim` can burn without the child
      signing (needs checking against the bearer model — a delegate is not an owner, so
      this may be compatible); or expose a "redeemable supply" figure that subtracts dead
      capabilities' balances, and be explicit that raw `supply` is not the solvency
      metric. Prefer the former if it holds up, since a number nobody has to know to
      interpret is worth more than a caveat.

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
