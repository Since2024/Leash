//! Proves the fix for docs/ROADMAP.md 0.3: one owner can hold more than one capability,
//! and the capabilities stay independent of each other.
//!
//! Before the fix, both `issue` and `attenuate` derived the Capability PDA at
//! `[CAPABILITY_SEED, owner]`. With no nonce in the seeds, a second `issue` for the same
//! principal collided with the first PDA and failed inside Anchor's `init` — one owner
//! pubkey could hold at most one capability, ever. `issue.rs` carried a literal
//! `// TODO: real seed scheme` at the line, and `attenuate.rs` documented the limitation
//! as deliberate, because leash-hook must re-derive "the Capability for this transfer"
//! from a *single* seed formula and two schemes cannot both be that formula.
//!
//! The fix gives each capability its own wrapped-token account at
//! `[TOKEN_ACCOUNT_SEED, owner, nonce]` and keys the Capability off *that address*. The
//! hook's formula stays single — it just points at base account 0 (the transfer's source
//! token account) instead of the owner slot, because the nonce is already folded into
//! that address and the Transfer Hook Interface supplies it on every transfer.
//!
//! What makes this file worth more than "two issues succeed": the interesting failure
//! mode isn't a collision, it's *bleed*. If the hook resolved the wrong capability for a
//! transfer, two capabilities held by one owner would share a budget — spending from one
//! would debit the other, and revoking one would kill both. Every test here pins that
//! down on the state, not just on the transaction outcome.
//!
//! Rejections are asserted by on-chain error code (`expect_err_code`), never bare
//! `is_err()` — see docs/ROADMAP.md 0.5 for why that distinction is load-bearing.

mod common;

use anchor_lang::prelude::Pubkey;
use anchor_spl::token_2022::spl_token_2022;
use common::*;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;

const FAR_FUTURE: i64 = 4_102_444_800; // 2100-01-01

/// Creates a merchant and its wrapped-token ATA, returning the ATA — the allowlisted
/// destination every spend in this file targets.
fn merchant_ata(svm: &mut LiteSVM, s: &Setup) -> Pubkey {
    let merchant = Keypair::new();
    let dest = ata(&merchant.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(
        svm,
        &s.payer,
        &[],
        &[create_ata_ix(
            &s.payer.pubkey(),
            &merchant.pubkey(),
            &s.wrapped_mint,
            &spl_token_2022::id(),
        )],
    )
    .unwrap();
    dest
}

/// The headline: the same principal issues twice. Pre-fix, the second call failed in
/// `init` because both capabilities derived to the same address.
#[test]
fn one_principal_can_hold_two_root_capabilities() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    let principal = Keypair::new();

    expect_ok(issue(&mut svm, &s, &principal, 0, 1_000, FAR_FUTURE, vec![dest]));
    expect_ok(issue(&mut svm, &s, &principal, 1, 500, FAR_FUTURE, vec![dest]));

    let (cap_a, ta_a) = capability_for(&principal.pubkey(), 0);
    let (cap_b, ta_b) = capability_for(&principal.pubkey(), 1);

    // Distinct accounts all the way down — different nonce, different token account,
    // therefore different capability.
    assert_ne!(ta_a, ta_b, "nonce must produce distinct token accounts");
    assert_ne!(cap_a, cap_b, "distinct token accounts must key distinct capabilities");

    // Each carries its own budget, and each token account holds exactly its own cap.
    assert_eq!(capability_state(&svm, &cap_a).cap, 1_000);
    assert_eq!(capability_state(&svm, &cap_b).cap, 500);
    assert_eq!(token_amount(&svm, &ta_a), 1_000);
    assert_eq!(token_amount(&svm, &ta_b), 500);

    // Both name the token account they were actually issued against. This field was
    // written and read by nothing before 0.4; the hook now checks it on every transfer.
    assert_eq!(capability_state(&svm, &cap_a).token_account, ta_a);
    assert_eq!(capability_state(&svm, &cap_b).token_account, ta_b);
}

/// Spending one capability must not debit the other. This is the test that would catch
/// the hook resolving the wrong capability for a transfer.
#[test]
fn spending_one_capability_does_not_debit_the_other() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    let principal = Keypair::new();
    expect_ok(issue(&mut svm, &s, &principal, 0, 1_000, FAR_FUTURE, vec![dest]));
    expect_ok(issue(&mut svm, &s, &principal, 1, 500, FAR_FUTURE, vec![dest]));

    let (cap_a, ta_a) = capability_for(&principal.pubkey(), 0);
    let (cap_b, ta_b) = capability_for(&principal.pubkey(), 1);

    expect_ok(spend(&mut svm, &s, &principal, &ta_a, &dest, 700));

    // A debits, B is untouched — in the capability's own accounting and in its balance.
    assert_eq!(capability_state(&svm, &cap_a).spent, 700);
    assert_eq!(capability_state(&svm, &cap_b).spent, 0);
    assert_eq!(token_amount(&svm, &ta_b), 500);

    // B can still spend its full budget. If the two shared accounting, 500 would now be
    // past the combined remainder and this would fail.
    expect_ok(spend(&mut svm, &s, &principal, &ta_b, &dest, 500));
    assert_eq!(capability_state(&svm, &cap_b).spent, 500);
    assert_eq!(capability_state(&svm, &cap_a).spent, 700);
}

/// Each capability's cap binds only itself. A's remaining budget is no help to B.
#[test]
fn each_capability_is_capped_independently() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    let principal = Keypair::new();
    expect_ok(issue(&mut svm, &s, &principal, 0, 1_000, FAR_FUTURE, vec![dest]));
    expect_ok(issue(&mut svm, &s, &principal, 1, 500, FAR_FUTURE, vec![dest]));

    let (_, ta_a) = capability_for(&principal.pubkey(), 0);
    let (cap_b, ta_b) = capability_for(&principal.pubkey(), 1);

    // Delegate 100 of B away. This is the only lever that makes a capability's *balance*
    // exceed its *remaining budget* — attenuation mints the child's units fresh, so B
    // still holds 500 while only 400 of it is spendable. Without that gap, an over-cap
    // spend is indistinguishable from Token-2022's own insufficient-funds rejection
    // (docs/ROADMAP.md 0.5) and the assertion below could not name an error code
    // honestly.
    let child = Keypair::new();
    expect_ok(attenuate(
        &mut svm, &s, &principal, cap_b, &child, 0, 100, FAR_FUTURE, vec![dest],
    ));

    // 401 is well within B's 500 balance, so Token-2022 is satisfied and the rejection
    // can only be leash-hook's. B is bound by its own budget, not by the 1_500 in wrapped
    // units its owner holds across both capabilities.
    expect_err_code(
        spend(&mut svm, &s, &principal, &ta_b, &dest, 401),
        "spending past the smaller capability's undelegated budget while the owner holds another",
        E_HOOK_CAP_EXCEEDED,
    );

    // Exactly on the line works — the bound is B's own, not a shared pool.
    expect_ok(spend(&mut svm, &s, &principal, &ta_b, &dest, 400));
    assert_eq!(capability_state(&svm, &cap_b).spent, 400);

    // And A is still fully spendable afterwards, untouched by any of B's accounting.
    expect_ok(spend(&mut svm, &s, &principal, &ta_a, &dest, 1_000));
}

/// Revocation is per-capability. Pre-fix this question could not even be posed, since an
/// owner had only one capability to revoke.
#[test]
fn revoking_one_capability_leaves_the_other_spendable() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    let principal = Keypair::new();
    expect_ok(issue(&mut svm, &s, &principal, 0, 1_000, FAR_FUTURE, vec![dest]));
    expect_ok(issue(&mut svm, &s, &principal, 1, 500, FAR_FUTURE, vec![dest]));

    let (cap_a, ta_a) = capability_for(&principal.pubkey(), 0);
    let (cap_b, ta_b) = capability_for(&principal.pubkey(), 1);

    expect_ok(revoke(&mut svm, &s, &principal, cap_a));
    assert!(capability_state(&svm, &cap_a).revoked);
    assert!(!capability_state(&svm, &cap_b).revoked);

    expect_err_code(
        spend(&mut svm, &s, &principal, &ta_a, &dest, 1),
        "spending a revoked capability",
        E_HOOK_REVOKED,
    );

    // The survivor is unaffected — revocation did not spill across the owner.
    expect_ok(spend(&mut svm, &s, &principal, &ta_b, &dest, 500));
    assert_eq!(capability_state(&svm, &cap_b).spent, 500);
}

/// One parent delegating to the *same* child owner twice. This is the `attenuate` half of
/// 0.3: pre-fix the second delegation collided on `[CAPABILITY_SEED, child_owner]`, so an
/// agent could never be given a second allowance.
#[test]
fn a_parent_can_delegate_to_the_same_owner_twice() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    let parent_owner = Keypair::new();
    expect_ok(issue(&mut svm, &s, &parent_owner, 0, 1_000, FAR_FUTURE, vec![dest]));
    let (parent_capability, _) = capability_for(&parent_owner.pubkey(), 0);

    // The same agent, twice, with different budgets.
    let agent = Keypair::new();
    expect_ok(attenuate(
        &mut svm,
        &s,
        &parent_owner,
        parent_capability,
        &agent,
        0,
        300,
        FAR_FUTURE,
        vec![dest],
    ));
    expect_ok(attenuate(
        &mut svm,
        &s,
        &parent_owner,
        parent_capability,
        &agent,
        1,
        200,
        FAR_FUTURE,
        vec![dest],
    ));

    let (child_a, child_ta_a) = capability_for(&agent.pubkey(), 0);
    let (child_b, child_ta_b) = capability_for(&agent.pubkey(), 1);
    assert_ne!(child_a, child_b);

    // Both reservations landed on the parent, and both children are real.
    assert_eq!(
        capability_state(&svm, &parent_capability).committed_to_children,
        500
    );
    assert_eq!(capability_state(&svm, &child_a).cap, 300);
    assert_eq!(capability_state(&svm, &child_b).cap, 200);
    assert_eq!(capability_state(&svm, &child_a).depth, 1);
    assert_eq!(capability_state(&svm, &child_b).depth, 1);

    // The agent's two allowances are enforced separately. This one asserts only that the
    // spend fails, not why: 201 also exceeds the account's 200 balance, so Token-2022
    // rejects it before leash-hook is consulted and no error code here would be
    // trustworthy (docs/ROADMAP.md 0.5). `each_capability_is_capped_independently` above
    // is the isolated version of this check.
    expect_err(
        spend(&mut svm, &s, &agent, &child_ta_b, &dest, 201),
        "spending past the second delegation's cap",
    );
    expect_ok(spend(&mut svm, &s, &agent, &child_ta_a, &dest, 300));
    expect_ok(spend(&mut svm, &s, &agent, &child_ta_b, &dest, 200));
    assert_eq!(capability_state(&svm, &child_a).spent, 300);
    assert_eq!(capability_state(&svm, &child_b).spent, 200);
}

/// Revocation is per-capability, and revocation authority is the capability's *own*
/// owner. Both halves matter once an agent can hold two allowances from one parent:
///
/// - The agent revoking one of its own leaves the other alive.
/// - The parent cannot reach in and revoke a single one of them. `revoke` is
///   `has_one = owner` (`revoke.rs:15`), so the parent's only lever is revoking its own
///   capability, which cascades to *every* descendant via the hook's ancestor walk.
///   Selective withdrawal of one delegation is therefore not expressible today — see
///   docs/ROADMAP.md 0.8.
#[test]
fn revoking_one_delegation_leaves_the_agents_other_one_alive() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    let parent_owner = Keypair::new();
    expect_ok(issue(&mut svm, &s, &parent_owner, 0, 1_000, FAR_FUTURE, vec![dest]));
    let (parent_capability, _) = capability_for(&parent_owner.pubkey(), 0);

    let agent = Keypair::new();
    for (nonce, cap) in [(0u64, 300u64), (1, 200)] {
        expect_ok(attenuate(
            &mut svm,
            &s,
            &parent_owner,
            parent_capability,
            &agent,
            nonce,
            cap,
            FAR_FUTURE,
            vec![dest],
        ));
    }

    let (child_a, child_ta_a) = capability_for(&agent.pubkey(), 0);
    let (child_b, child_ta_b) = capability_for(&agent.pubkey(), 1);

    // The parent cannot revoke one of the agent's capabilities directly — it does not own
    // it. Its only lever is its own capability, which would cut off both.
    expect_err_code(
        revoke(&mut svm, &s, &parent_owner, child_a),
        "parent revoking a child capability it does not own",
        E_LEASH_UNAUTHORIZED,
    );
    assert!(!capability_state(&svm, &child_a).revoked);

    // The holder revoking its own capability works, and binds only that one.
    expect_ok(revoke(&mut svm, &s, &agent, child_a));
    assert!(capability_state(&svm, &child_a).revoked);
    assert!(!capability_state(&svm, &child_b).revoked);

    expect_err_code(
        spend(&mut svm, &s, &agent, &child_ta_a, &dest, 1),
        "spending the revoked delegation",
        E_HOOK_REVOKED,
    );
    expect_ok(spend(&mut svm, &s, &agent, &child_ta_b, &dest, 200));
    assert_eq!(capability_state(&svm, &child_b).spent, 200);
}

/// The 0.4 half, made concrete: a capability's budget is bound to *its own* token
/// account, so units held elsewhere by the same owner are not spendable against it.
///
/// Here the agent holds a delegated capability *and* a merchant-style balance received
/// through a normal transfer. The received units sit in a plain ATA with no capability
/// keyed to it, so no capability's budget backs them — the hook finds nothing to debit
/// and the transfer is rejected rather than silently charged to the agent's capability.
#[test]
fn units_outside_a_capabilitys_own_account_are_not_its_budget() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    // The agent's plain ATA has to be on the parent's allowlist, since the parent pays it
    // like any other destination further down.
    let agent = Keypair::new();
    let agent_plain_ata = ata(&agent.pubkey(), &s.wrapped_mint, &spl_token_2022::id());

    let parent_owner = Keypair::new();
    expect_ok(issue(
        &mut svm,
        &s,
        &parent_owner,
        0,
        1_000,
        FAR_FUTURE,
        vec![dest, agent_plain_ata],
    ));
    let (parent_capability, parent_ta) = capability_for(&parent_owner.pubkey(), 0);

    // The agent gets a 200 delegated capability...
    expect_ok(attenuate(
        &mut svm,
        &s,
        &parent_owner,
        parent_capability,
        &agent,
        0,
        200,
        FAR_FUTURE,
        vec![dest],
    ));
    let (agent_capability, agent_ta) = capability_for(&agent.pubkey(), 0);

    // ...and separately receives 100 units into a plain ATA, the way a merchant would.
    send(
        &mut svm,
        &s.payer,
        &[],
        &[create_ata_ix(
            &s.payer.pubkey(),
            &agent.pubkey(),
            &s.wrapped_mint,
            &spl_token_2022::id(),
        )],
    )
    .unwrap();
    expect_ok(spend(
        &mut svm,
        &s,
        &parent_owner,
        &parent_ta,
        &agent_plain_ata,
        100,
    ));
    assert_eq!(token_amount(&svm, &agent_plain_ata), 100);

    // The capability's own budget is untouched by that receipt: still 200, still 0 spent.
    assert_eq!(capability_state(&svm, &agent_capability).cap, 200);
    assert_eq!(capability_state(&svm, &agent_capability).spent, 0);
    assert_eq!(token_amount(&svm, &agent_ta), 200);

    // Spending from the capability's own account works and debits the capability.
    expect_ok(spend(&mut svm, &s, &agent, &agent_ta, &dest, 200));
    assert_eq!(capability_state(&svm, &agent_capability).spent, 200);

    // The 100 in the plain ATA is *not* additional capability budget. There is no
    // capability at `[CAPABILITY_SEED, agent_plain_ata]`, so the hook cannot resolve one
    // and the transfer dies in account resolution rather than being charged to the
    // agent's now-exhausted capability.
    expect_err(
        spend(&mut svm, &s, &agent, &agent_plain_ata, &dest, 100),
        "spending received units as if they were capability budget",
    );

    // Whatever happened above, it did not move the capability's accounting.
    assert_eq!(capability_state(&svm, &agent_capability).spent, 200);
}
