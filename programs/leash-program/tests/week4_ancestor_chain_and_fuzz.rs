//! Week 4 (docs/BUILD_PLAN.md §7/§8): the remaining items from the invariant checklist
//! (tests/invariants/README.md) — the ancestor chain beyond one level, attenuate's
//! conservation/depth checks, and the expiry boundary.

mod common;
use common::*;

use anchor_lang::solana_program::clock::Clock;
use anchor_lang::prelude::Pubkey;
use anchor_spl::token_2022::spl_token_2022;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;

const FAR_FUTURE: i64 = 9_999_999_999;

/// Builds root -> A -> B -> C (depths 0/1/2/3) with a shared allowlist, and airdrops/
/// funds everything needed. Returns the owners and capability addresses at each level.
struct Chain {
    root_owner: Keypair,
    root_capability: Pubkey,
    a_owner: Keypair,
    a_capability: Pubkey,
    b_owner: Keypair,
    b_capability: Pubkey,
    c_owner: Keypair,
    c_capability: Pubkey,
    c_token_account: Pubkey,
}

fn build_depth3_chain(svm: &mut LiteSVM, s: &Setup, merchant_ata: Pubkey) -> Chain {
    let root_owner = Keypair::new();
    let (root_capability, _) = capability_for(&root_owner.pubkey(), 0);
    expect_ok(issue(svm, s, &root_owner, 0, 1_000, FAR_FUTURE, vec![merchant_ata]));

    let a_owner = Keypair::new();
    let (a_capability, _) = capability_for(&a_owner.pubkey(), 0);
    expect_ok(attenuate(svm, s, &root_owner, root_capability, &a_owner, 0, 500, FAR_FUTURE, vec![merchant_ata]));

    let b_owner = Keypair::new();
    let (b_capability, _) = capability_for(&b_owner.pubkey(), 0);
    expect_ok(attenuate(svm, s, &a_owner, a_capability, &b_owner, 0, 200, FAR_FUTURE, vec![merchant_ata]));

    let c_owner = Keypair::new();
    let (c_capability, c_token_account) = capability_for(&c_owner.pubkey(), 0);
    expect_ok(attenuate(svm, s, &b_owner, b_capability, &c_owner, 0, 50, FAR_FUTURE, vec![merchant_ata]));

    assert_eq!(capability_state(svm, &c_capability).depth, 3);
    assert_eq!(capability_state(svm, &c_capability).ancestors, [b_capability, a_capability, root_capability]);

    Chain {
        root_owner,
        root_capability,
        a_owner,
        a_capability,
        b_owner,
        b_capability,
        c_owner,
        c_capability,
        c_token_account,
    }
}

#[test]
fn depth3_spend_succeeds_when_nothing_revoked() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let merchant_owner = Keypair::new();
    let merchant_ata = ata(&merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(&mut svm, &s.payer, &[], &[create_ata_ix(&s.payer.pubkey(), &merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id())]).unwrap();

    let chain = build_depth3_chain(&mut svm, &s, merchant_ata);
    expect_ok(spend(&mut svm, &s, &chain.c_owner, &chain.c_token_account, &merchant_ata, 10));
    assert_eq!(capability_state(&svm, &chain.c_capability).spent, 10);
}

#[test]
fn depth3_spend_fails_when_immediate_parent_revoked() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let merchant_owner = Keypair::new();
    let merchant_ata = ata(&merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(&mut svm, &s.payer, &[], &[create_ata_ix(&s.payer.pubkey(), &merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id())]).unwrap();

    let chain = build_depth3_chain(&mut svm, &s, merchant_ata);
    // B is C's immediate parent (ancestors[0]).
    revoke(&mut svm, &s, &chain.b_owner, chain.b_capability).unwrap();
    expect_err(
        spend(&mut svm, &s, &chain.c_owner, &chain.c_token_account, &merchant_ata, 10),
        "depth-3 spend after immediate parent (B) revoked",
    );
}

#[test]
fn depth3_spend_fails_when_grandparent_revoked() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let merchant_owner = Keypair::new();
    let merchant_ata = ata(&merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(&mut svm, &s.payer, &[], &[create_ata_ix(&s.payer.pubkey(), &merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id())]).unwrap();

    let chain = build_depth3_chain(&mut svm, &s, merchant_ata);
    // A is C's grandparent (ancestors[1]). B (the immediate parent) stays live, so this
    // isolates that the *second* ancestor slot is actually being checked, not just the
    // first.
    revoke(&mut svm, &s, &chain.a_owner, chain.a_capability).unwrap();
    expect_err(
        spend(&mut svm, &s, &chain.c_owner, &chain.c_token_account, &merchant_ata, 10),
        "depth-3 spend after grandparent (A) revoked",
    );
}

#[test]
fn depth3_spend_fails_when_root_revoked() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let merchant_owner = Keypair::new();
    let merchant_ata = ata(&merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(&mut svm, &s.payer, &[], &[create_ata_ix(&s.payer.pubkey(), &merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id())]).unwrap();

    let chain = build_depth3_chain(&mut svm, &s, merchant_ata);
    // root is C's great-grandparent (ancestors[2]) — the deepest slot MAX_DEPTH allows.
    // A and B (the two closer ancestors) stay live, isolating that the *third* slot is
    // genuinely checked, not just the first two.
    revoke(&mut svm, &s, &chain.root_owner, chain.root_capability).unwrap();
    expect_err(
        spend(&mut svm, &s, &chain.c_owner, &chain.c_token_account, &merchant_ata, 10),
        "depth-3 spend after root (great-grandparent) revoked",
    );
}

#[test]
fn attenuate_rejects_child_cap_exceeding_parent_remaining() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let merchant_owner = Keypair::new();
    let merchant_ata = ata(&merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(&mut svm, &s.payer, &[], &[create_ata_ix(&s.payer.pubkey(), &merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id())]).unwrap();

    let parent_owner = Keypair::new();
    let (parent_capability, _) = capability_for(&parent_owner.pubkey(), 0);
    expect_ok(issue(&mut svm, &s, &parent_owner, 0, 100, FAR_FUTURE, vec![merchant_ata]));

    // Exactly at the boundary: succeeds.
    let child_ok_owner = Keypair::new();
    expect_ok(attenuate(&mut svm, &s, &parent_owner, parent_capability, &child_ok_owner, 0, 100, FAR_FUTURE, vec![merchant_ata]));
    assert_eq!(capability_state(&svm, &parent_capability).committed_to_children, 100);

    // One unit over the now-fully-committed parent: fails.
    let child_over_owner = Keypair::new();
    expect_err(
        attenuate(&mut svm, &s, &parent_owner, parent_capability, &child_over_owner, 0, 1, FAR_FUTURE, vec![merchant_ata]),
        "attenuate exceeding parent's remaining budget",
    );
}

#[test]
fn attenuate_rejects_depth_past_max_depth() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let merchant_owner = Keypair::new();
    let merchant_ata = ata(&merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(&mut svm, &s.payer, &[], &[create_ata_ix(&s.payer.pubkey(), &merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id())]).unwrap();

    // Build a depth-3 chain (root=0 -> A=1 -> B=2 -> C=3), which is exactly MAX_DEPTH.
    let chain = build_depth3_chain(&mut svm, &s, merchant_ata);
    assert_eq!(capability_state(&svm, &chain.c_capability).depth, 3);

    // Attenuating from C (depth 3, == MAX_DEPTH) must fail: attenuate requires
    // parent.depth < MAX_DEPTH.
    let too_deep_owner = Keypair::new();
    expect_err(
        attenuate(&mut svm, &s, &chain.c_owner, chain.c_capability, &too_deep_owner, 0, 1, FAR_FUTURE, vec![merchant_ata]),
        "attenuate past MAX_DEPTH",
    );
}

#[test]
fn spend_respects_expiry_boundary() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let merchant_owner = Keypair::new();
    let merchant_ata = ata(&merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(&mut svm, &s.payer, &[], &[create_ata_ix(&s.payer.pubkey(), &merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id())]).unwrap();

    let mut clock: Clock = svm.get_sysvar();
    let expiry = clock.unix_timestamp + 100;

    let principal = Keypair::new();
    let (_, principal_ta) = capability_for(&principal.pubkey(), 0);
    expect_ok(issue(&mut svm, &s, &principal, 0, 500, expiry, vec![merchant_ata]));

    // Before expiry: succeeds.
    expect_ok(spend(&mut svm, &s, &principal, &principal_ta, &merchant_ata, 10));

    // Warp the clock to one second after expiry, then retry (different amount, so this
    // isn't a byte-for-byte duplicate of the prior transaction — see week3's note on why
    // that would produce a false-positive `AlreadyProcessed` rejection instead of a real
    // one).
    clock.unix_timestamp = expiry + 1;
    svm.set_sysvar(&clock);
    expect_err(
        spend(&mut svm, &s, &principal, &principal_ta, &merchant_ata, 11),
        "spend one second after expiry",
    );
}
