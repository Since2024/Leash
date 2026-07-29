//! Proves the conservation invariant the `Capability` struct advertises and BUILD_PLAN §2
//! promises:
//!
//! ```text
//! spent + committed_to_children <= cap
//! ```
//!
//! It did not hold. `attenuate` reserved budget into `committed_to_children`, but neither
//! enforcement point subtracted it — `leash-hook`'s `spend_logic` and `record_spend` both
//! checked only `spent + amount <= cap`. Because attenuation *mints* the child's units
//! rather than moving the parent's, the parent's token account still held its full
//! original balance, so nothing stopped a parent from spending budget it had already
//! delegated. Parent and child could each spend the same units, putting more into
//! circulation than the vault backs. See docs/ROADMAP.md 0.2.
//!
//! Every rejection here is asserted by its specific on-chain error code via
//! `expect_err_code`, not by `is_err()`. That matters more than usual for this file: the
//! over-delegation rejection and Token-2022's own insufficient-funds rejection are
//! observably identical to a bare `is_err()` check, so a weaker assertion could pass
//! while the bug is fully intact (docs/ROADMAP.md 0.5).

mod common;

use common::*;
use anchor_spl::token_2022::spl_token_2022;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;

const FAR_FUTURE: i64 = 4_102_444_800; // 2100-01-01

/// The double-spend, end to end. Without the fix, step 3 succeeds and the tree spends
/// 1_400 against a 1_000 deposit.
#[test]
fn parent_cannot_spend_budget_already_delegated_to_a_child() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);

    let merchant = Keypair::new();
    let merchant_ata = ata(&merchant.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(
        &mut svm,
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

    // 1. Root capability: cap 1_000, backed by a 1_000 deposit into the vault.
    let parent_owner = Keypair::new();
    expect_ok(issue(
        &mut svm,
        &s,
        &parent_owner,
        1_000,
        FAR_FUTURE,
        vec![merchant_ata],
    ));
    let parent_capability = capability_pda(&parent_owner.pubkey());

    // 2. Delegate 400 of it to a child. This mints 400 *fresh* units to the child and
    //    reserves 400 on the parent — the parent's own token account is untouched and
    //    still holds all 1_000.
    let child_owner = Keypair::new();
    expect_ok(attenuate(
        &mut svm,
        &s,
        &parent_owner,
        parent_capability,
        &child_owner,
        400,
        FAR_FUTURE,
        vec![merchant_ata],
    ));
    let child_capability = capability_pda(&child_owner.pubkey());

    let parent = capability_state(&svm, &parent_capability);
    assert_eq!(parent.cap, 1_000);
    assert_eq!(parent.spent, 0);
    assert_eq!(parent.committed_to_children, 400);
    // The parent physically holds more than it may spend. This gap is the bug's fuel.
    assert_eq!(
        token_amount(
            &svm,
            &ata(&parent_owner.pubkey(), &s.wrapped_mint, &spl_token_2022::id())
        ),
        1_000
    );

    // 3. THE BUG. 601 is within `cap - spent` (1_000) but past `cap - spent - committed`
    //    (600). Pre-fix, the hook computed `0 + 601 <= 1_000` and allowed it.
    expect_err_code(
        spend(&mut svm, &s, &parent_owner, &merchant_ata, 601),
        "parent spending into budget already delegated to its child",
        E_HOOK_CAP_EXCEEDED,
    );
    // Nothing was written: the rejection happened before `record_spend`.
    assert_eq!(capability_state(&svm, &parent_capability).spent, 0);

    // 4. Exactly the free budget still works — the fix bounds the parent, it doesn't
    //    freeze it. (600, not 601, so this is a genuinely distinct transaction and
    //    cannot pass as an `AlreadyProcessed` artifact of the call above.)
    expect_ok(spend(&mut svm, &s, &parent_owner, &merchant_ata, 600));
    assert_eq!(capability_state(&svm, &parent_capability).spent, 600);

    // 5. The child's delegated budget is still fully intact and independently spendable —
    //    the parent spending its own share did not eat into the child's.
    expect_ok(spend(&mut svm, &s, &child_owner, &merchant_ata, 400));
    assert_eq!(capability_state(&svm, &child_capability).spent, 400);

    // 6. Conservation holds at every node, and the tree spent exactly the deposit that
    //    backs it — 600 + 400 = 1_000, not 1_400.
    let parent = capability_state(&svm, &parent_capability);
    let child = capability_state(&svm, &child_capability);
    assert!(parent.spent + parent.committed_to_children <= parent.cap);
    assert!(child.spent + child.committed_to_children <= child.cap);
    assert_eq!(parent.spent + child.spent, 1_000);
    assert_eq!(token_amount(&svm, &merchant_ata), 1_000);

    // 7. And the parent is now genuinely exhausted: its remaining free budget is zero,
    //    even though its token account still shows a 400 balance (those units are the
    //    child's, minted against the same deposit).
    expect_err_code(
        spend(&mut svm, &s, &parent_owner, &merchant_ata, 1),
        "parent spending after exhausting its non-delegated budget",
        E_HOOK_CAP_EXCEEDED,
    );
}

/// The boundary, isolated from the walk-through above: with 400 of a 1_000 cap delegated,
/// 600 is allowed and 601 is not.
#[test]
fn delegated_budget_boundary_is_exact() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);

    let merchant = Keypair::new();
    let merchant_ata = ata(&merchant.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(
        &mut svm,
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

    let parent_owner = Keypair::new();
    expect_ok(issue(
        &mut svm,
        &s,
        &parent_owner,
        1_000,
        FAR_FUTURE,
        vec![merchant_ata],
    ));
    let parent_capability = capability_pda(&parent_owner.pubkey());

    let child_owner = Keypair::new();
    expect_ok(attenuate(
        &mut svm,
        &s,
        &parent_owner,
        parent_capability,
        &child_owner,
        400,
        FAR_FUTURE,
        vec![merchant_ata],
    ));

    // One over the line.
    expect_err_code(
        spend(&mut svm, &s, &parent_owner, &merchant_ata, 601),
        "spend one unit past the undelegated budget",
        E_HOOK_CAP_EXCEEDED,
    );
    // Exactly on it.
    expect_ok(spend(&mut svm, &s, &parent_owner, &merchant_ata, 600));

    let parent = capability_state(&svm, &parent_capability);
    assert_eq!(parent.spent, 600);
    assert_eq!(parent.spent + parent.committed_to_children, parent.cap);
}

/// Successive attenuations accumulate: two children of 300 each leave 400 spendable, not
/// 700. Guards against the reservation being read as "the most recent child" rather than
/// a running total.
#[test]
fn multiple_children_reservations_accumulate() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);

    let merchant = Keypair::new();
    let merchant_ata = ata(&merchant.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(
        &mut svm,
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

    let parent_owner = Keypair::new();
    expect_ok(issue(
        &mut svm,
        &s,
        &parent_owner,
        1_000,
        FAR_FUTURE,
        vec![merchant_ata],
    ));
    let parent_capability = capability_pda(&parent_owner.pubkey());

    for child_cap in [300u64, 300] {
        let child_owner = Keypair::new();
        expect_ok(attenuate(
            &mut svm,
            &s,
            &parent_owner,
            parent_capability,
            &child_owner,
            child_cap,
            FAR_FUTURE,
            vec![merchant_ata],
        ));
    }
    assert_eq!(
        capability_state(&svm, &parent_capability).committed_to_children,
        600
    );

    expect_err_code(
        spend(&mut svm, &s, &parent_owner, &merchant_ata, 401),
        "spend past the sum of both children's reservations",
        E_HOOK_CAP_EXCEEDED,
    );
    expect_ok(spend(&mut svm, &s, &parent_owner, &merchant_ata, 400));

    let parent = capability_state(&svm, &parent_capability);
    assert_eq!(parent.spent + parent.committed_to_children, parent.cap);
}
