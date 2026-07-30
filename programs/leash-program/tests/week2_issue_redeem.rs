//! Week 2 (docs/BUILD_PLAN.md §7): a full deposit -> issue -> redeem round trip against
//! real Anchor instructions (not stubs), using LiteSVM.
//!
//! Note: `issue`/`redeem` only ever mint_to/transfer/burn — neither touches a transfer,
//! so leash-hook's TransferHook is never invoked here and doesn't need to be loaded in
//! this SVM. It only fires on an actual `transfer`/`transfer_checked` of the wrapped
//! mint, which is Week 3's `spend` path, not this one.

mod common;
use common::*;

use anchor_lang::prelude::Pubkey;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;

#[test]
fn deposit_issue_redeem_round_trip() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);

    let principal = Keypair::new();
    let (capability, capability_token_account) = capability_for(&principal.pubkey(), 0);

    // --- issue: deposit 400, get a $400 capability. ---
    expect_ok(issue(&mut svm, &s, &principal, 0, 400, 9_999_999_999, vec![]));

    assert_eq!(token_amount(&svm, &s.vault), 400);
    assert_eq!(token_amount(&svm, &capability_token_account), 400);
    let principal_usdc = ata(&principal.pubkey(), &s.usdc_mint, &anchor_spl::token::spl_token::id());
    assert_eq!(token_amount(&svm, &principal_usdc), 0); // deposited all of it

    let cap_state = capability_state(&svm, &capability);
    assert_eq!(cap_state.owner, principal.pubkey());
    assert_eq!(cap_state.parent, Pubkey::default()); // root: no parent (state.rs)
    assert_eq!(cap_state.ancestors, [Pubkey::default(); 3]); // root: no ancestors either
    assert_eq!(cap_state.cap, 400);
    assert_eq!(cap_state.spent, 0);
    assert_eq!(cap_state.committed_to_children, 0);
    assert!(!cap_state.revoked);
    assert_eq!(cap_state.depth, 0);

    // --- redeem: cash 150 of the $400 back out. ---
    expect_ok(redeem(&mut svm, &s, &principal, capability_token_account, principal_usdc, 150));

    assert_eq!(token_amount(&svm, &s.vault), 250);
    assert_eq!(token_amount(&svm, &capability_token_account), 250);
    assert_eq!(token_amount(&svm, &principal_usdc), 150);
}
