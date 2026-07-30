//! Week 3 (docs/BUILD_PLAN.md §7): the actual point of the project. `attenuate` plus
//! real spend-path enforcement in `leash-hook` — cap, expiry, allowlist, revoked, and a
//! single ancestor level of revoked. Supersedes Week 1's placeholder spike test (removed):
//! this exercises the same transfer-hook invocation mechanism, but with real per-source
//! Capability derivation and real business-rule enforcement instead of a fixed marker.

mod common;
use common::*;

use anchor_spl::token_2022::spl_token_2022;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;

#[test]
fn spend_enforcement_and_attenuate() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);

    let principal = Keypair::new();
    let merchant_owner = Keypair::new();
    let merchant_ata = ata(&merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(&mut svm, &s.payer, &[], &[create_ata_ix(&s.payer.pubkey(), &merchant_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id())]).unwrap();

    let not_allowlisted_owner = Keypair::new();
    let not_allowlisted_ata = ata(&not_allowlisted_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(&mut svm, &s.payer, &[], &[create_ata_ix(&s.payer.pubkey(), &not_allowlisted_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id())]).unwrap();

    let (capability, principal_ta) = capability_for(&principal.pubkey(), 0);
    let far_future = 9_999_999_999;
    expect_ok(issue(&mut svm, &s, &principal, 0, 1_000, far_future, vec![merchant_ata]));

    // --- Valid spend: within cap, allowlisted, not expired, not revoked. ---
    expect_ok(spend(&mut svm, &s, &principal, &principal_ta, &merchant_ata, 100));
    assert_eq!(capability_state(&svm, &capability).spent, 100);
    assert_eq!(token_amount(&svm, &merchant_ata), 100);

    // --- Not allowlisted: rejected. ---
    expect_err(
        spend(&mut svm, &s, &principal, &principal_ta, &not_allowlisted_ata, 50),
        "spend to non-allowlisted destination",
    );
    assert_eq!(capability_state(&svm, &capability).spent, 100); // unchanged

    // --- Exceeds remaining cap (900 left, try 901): rejected. Honest caveat: in this
    // MVP, the wrapped token account's actual balance always equals the capability's
    // remaining cap (we only ever mint exactly `cap` units, once), so this rejection is
    // observably the same whether it comes from leash-hook's own `amount + spent > cap`
    // check or from Token-2022's native insufficient-balance check firing first. This
    // test proves "spending more than issued fails," not specifically "leash-hook's cap
    // check works" in isolation from that overlap — a real gap, not swept under a
    // green checkmark. Isolating the two would need a scenario where token balance
    // exceeds the capability's cap, which nothing in the current instruction set
    // produces.
    expect_err(
        spend(&mut svm, &s, &principal, &principal_ta, &merchant_ata, 901),
        "spend exceeding cap",
    );
    assert_eq!(capability_state(&svm, &capability).spent, 100); // unchanged

    // --- Exactly at the remaining cap boundary: succeeds. ---
    expect_ok(spend(&mut svm, &s, &principal, &principal_ta, &merchant_ata, 900));
    assert_eq!(capability_state(&svm, &capability).spent, 1_000);

    // --- Revoke, then spend (even 0 remaining room aside): rejected specifically
    // because of `revoked`, not because the cap is exhausted — verified by revoking a
    // *fresh* capability with room left. ---
    let principal2 = Keypair::new();
    let (capability2, principal2_ta) = capability_for(&principal2.pubkey(), 0);
    expect_ok(issue(&mut svm, &s, &principal2, 0, 500, far_future, vec![merchant_ata]));
    expect_ok(spend(&mut svm, &s, &principal2, &principal2_ta, &merchant_ata, 10)); // works before revoke

    revoke(&mut svm, &s, &principal2, capability2).unwrap();
    // Different amount than the pre-revoke spend above (10) — an identical transaction
    // would be rejected as `AlreadyProcessed` by LiteSVM regardless of leash-hook's
    // logic, which would make this assertion pass for the wrong reason (caught by
    // actually reading the failure reason during development, not by inspection).
    expect_err(
        spend(&mut svm, &s, &principal2, &principal2_ta, &merchant_ata, 11),
        "spend after revoke",
    );

    // --- Single ancestor level: attenuate a child from principal3, revoke the PARENT,
    // confirm the CHILD's spend is rejected too. ---
    let principal3 = Keypair::new();
    let (capability3, _principal3_ta) = capability_for(&principal3.pubkey(), 0);
    expect_ok(issue(&mut svm, &s, &principal3, 0, 500, far_future, vec![merchant_ata]));

    let child_owner = Keypair::new();
    let (child_capability, child_ta) = capability_for(&child_owner.pubkey(), 0);
    expect_ok(attenuate(
        &mut svm,
        &s,
        &principal3,
        capability3,
        &child_owner,
        0,
        200,
        far_future,
        vec![merchant_ata],
    ));
    assert_eq!(capability_state(&svm, &capability3).committed_to_children, 200);
    assert_eq!(capability_state(&svm, &child_capability).depth, 1);
    assert_eq!(capability_state(&svm, &child_capability).parent, capability3);

    // Child can spend fine while parent is live.
    expect_ok(spend(&mut svm, &s, &child_owner, &child_ta, &merchant_ata, 10));

    // Revoke the parent (capability3); child's own `revoked` flag is untouched, but the
    // ancestor check in leash-hook must still reject the child's spend.
    revoke(&mut svm, &s, &principal3, capability3).unwrap();
    assert!(!capability_state(&svm, &child_capability).revoked); // child's own flag: untouched
    // A *different* amount than the prior child spend (10) — see the comment above on
    // why an identical transaction would be a false-positive `AlreadyProcessed` failure.
    expect_err(
        spend(&mut svm, &s, &child_owner, &child_ta, &merchant_ata, 11),
        "child spend after parent revoked",
    );
}
