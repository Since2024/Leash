//! Proves `redeem` can no longer be used to walk around the transfer hook
//! (docs/ROADMAP.md 0.1).
//!
//! Burning does not fire a Token-2022 transfer hook, and `redeem` originally never
//! referenced a `Capability` at all — so a delegated agent could burn its *unspent
//! budget* and receive unrestricted real USDC at an address of its choosing. The
//! allowlist and revocation both applied only to transfers, and redemption was not a
//! transfer. "Give your agent $20 and it physically cannot exceed them" held for
//! spending and not for cashing out.
//!
//! The rule now enforced, by who actually funded the vault:
//!
//! - A merchant (no capability) may redeem — the units were earned through a real,
//!   hook-checked transfer.
//! - A root capability's owner may redeem, bounded by
//!   `cap - spent - committed_to_children`, with `cap` shrinking to match.
//! - A delegated (child) capability may NOT redeem; it never deposited anything.
//!
//! Rejections assert their specific on-chain error code (docs/ROADMAP.md 0.5): the
//! interesting failure here and a plain Token-2022 insufficient-funds failure look
//! identical to `is_err()`, so a weaker assertion could pass with the hole wide open.

mod common;

use common::*;

use anchor_spl::token::spl_token;
use anchor_spl::token_2022::spl_token_2022;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;

const FAR_FUTURE: i64 = 4_102_444_800; // 2100-01-01

/// Gives `who` a USDC account to receive redemptions into.
fn usdc_account(svm: &mut LiteSVM, s: &Setup, who: &Keypair) -> anchor_lang::prelude::Pubkey {
    let acct = ata(&who.pubkey(), &s.usdc_mint, &spl_token::id());
    send(
        svm,
        &s.payer,
        &[],
        &[create_ata_ix(
            &s.payer.pubkey(),
            &who.pubkey(),
            &s.usdc_mint,
            &spl_token::id(),
        )],
    )
    .unwrap();
    acct
}

/// THE HOLE. A delegated agent holding unspent budget tries to cash it out to an address
/// of its own choosing, bypassing the allowlist entirely. Pre-fix this succeeded and the
/// agent walked away with real USDC.
#[test]
fn delegated_capability_cannot_redeem_its_budget() {
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

    // Principal deposits 1_000 and delegates 200 to an agent. Note the allowlist contains
    // only the merchant — the agent may pay that merchant and nobody else.
    let principal = Keypair::new();
    expect_ok(issue(
        &mut svm,
        &s,
        &principal,
        0,
        1_000,
        FAR_FUTURE,
        vec![merchant_ata],
    ));
    let (principal_capability, _principal_ta) = capability_for(&principal.pubkey(), 0);

    let agent = Keypair::new();
    expect_ok(attenuate(
        &mut svm,
        &s,
        &principal,
        principal_capability,
        &agent,
        0,
        200,
        FAR_FUTURE,
        vec![merchant_ata],
    ));

    let agent_wrapped = token_account_pda(&agent.pubkey(), 0);
    let agent_usdc = usdc_account(&mut svm, &s, &agent);
    assert_eq!(token_amount(&svm, &agent_wrapped), 200);

    // The attack: burn the delegated budget, take real USDC somewhere never allowlisted.
    expect_err_code(
        redeem(&mut svm, &s, &agent, agent_wrapped, agent_usdc, 200),
        "delegated agent cashing out its unspent budget",
        E_LEASH_DELEGATED_CANNOT_REDEEM,
    );
    assert_eq!(token_amount(&svm, &agent_usdc), 0);
    assert_eq!(token_amount(&svm, &agent_wrapped), 200); // nothing burned
    assert_eq!(token_amount(&svm, &s.vault), 1_000); // vault untouched

    // Revocation doesn't open a side door either — a revoked capability is *more*
    // restricted, not less.
    expect_ok(revoke(
        &mut svm,
        &s,
        &agent,
        capability_for(&agent.pubkey(), 0).0,
    ));
    expect_err_code(
        redeem(&mut svm, &s, &agent, agent_wrapped, agent_usdc, 150),
        "revoked delegated agent cashing out",
        E_LEASH_DELEGATED_CANNOT_REDEEM,
    );

    // What the agent *can* still do is spend to the allowlisted merchant — the capability
    // still works as a capability. (Re-issued to a fresh agent, since the one above just
    // revoked itself.)
    let agent2 = Keypair::new();
    expect_ok(attenuate(
        &mut svm,
        &s,
        &principal,
        principal_capability,
        &agent2,
        0,
        100,
        FAR_FUTURE,
        vec![merchant_ata],
    ));
    expect_ok(spend(&mut svm, &s, &agent2, &token_account_pda(&agent2.pubkey(), 0), &merchant_ata, 100));
    assert_eq!(token_amount(&svm, &merchant_ata), 100);
}

/// A merchant who was paid through the hook can still cash out — that is what makes
/// accepting Leash as good as accepting cash, and it must not be collateral damage.
#[test]
fn merchant_can_still_redeem_what_it_was_paid() {
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
    let merchant_usdc = usdc_account(&mut svm, &s, &merchant);

    let principal = Keypair::new();
    let (_, principal_ta) = capability_for(&principal.pubkey(), 0);
    expect_ok(issue(
        &mut svm,
        &s,
        &principal,
        0,
        500,
        FAR_FUTURE,
        vec![merchant_ata],
    ));

    expect_ok(spend(&mut svm, &s, &principal, &principal_ta, &merchant_ata, 300));
    assert_eq!(token_amount(&svm, &merchant_ata), 300);

    // The merchant holds no capability at all, so redemption is unrestricted.
    expect_ok(redeem(
        &mut svm,
        &s,
        &merchant,
        merchant_ata,
        merchant_usdc,
        300,
    ));
    assert_eq!(token_amount(&svm, &merchant_usdc), 300);
    assert_eq!(token_amount(&svm, &merchant_ata), 0);
    assert_eq!(token_amount(&svm, &s.vault), 200);
}

/// The depositor may unwind their own root capability — but only down to the collateral
/// that isn't already promised to a child, and `cap` shrinks to match what's left.
#[test]
fn root_redemption_is_bounded_by_committed_budget() {
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

    let principal = Keypair::new();
    expect_ok(issue(
        &mut svm,
        &s,
        &principal,
        0,
        1_000,
        FAR_FUTURE,
        vec![merchant_ata],
    ));
    let (principal_capability, principal_ta) = capability_for(&principal.pubkey(), 0);
    let principal_wrapped = principal_ta;
    let principal_usdc = ata(&principal.pubkey(), &s.usdc_mint, &spl_token::id());

    // Delegate 400, then spend 100. Free budget is 1_000 - 100 - 400 = 500.
    let agent = Keypair::new();
    expect_ok(attenuate(
        &mut svm,
        &s,
        &principal,
        principal_capability,
        &agent,
        0,
        400,
        FAR_FUTURE,
        vec![merchant_ata],
    ));
    expect_ok(spend(&mut svm, &s, &principal, &principal_ta, &merchant_ata, 100));

    // Redeeming past the free budget would drain collateral the agent's 400 units are
    // minted against — the docs/ROADMAP.md 0.2 shortfall, reached via redemption.
    expect_err_code(
        redeem(
            &mut svm,
            &s,
            &principal,
            principal_wrapped,
            principal_usdc,
            501,
        ),
        "root redeeming past its uncommitted budget",
        E_LEASH_CAP_EXCEEDED,
    );

    // Exactly the free budget is fine, and shrinks `cap` to match the remaining
    // collateral: 1_000 - 500 = 500, which is still 100 spent + 400 committed.
    expect_ok(redeem(
        &mut svm,
        &s,
        &principal,
        principal_wrapped,
        principal_usdc,
        500,
    ));
    let cap_state = capability_state(&svm, &principal_capability);
    assert_eq!(cap_state.cap, 500);
    assert_eq!(cap_state.spent, 100);
    assert_eq!(cap_state.committed_to_children, 400);
    assert!(cap_state.spent + cap_state.committed_to_children <= cap_state.cap);
    assert_eq!(token_amount(&svm, &principal_usdc), 500);

    // The vault still fully backs everything still outstanding: the agent's 400 unspent
    // units and the 100 already paid to the merchant.
    assert_eq!(token_amount(&svm, &s.vault), 500);

    // And having redeemed its free budget, the root can no longer spend — `cap` came
    // down with it rather than leaving phantom spending power behind.
    expect_err_code(
        spend(&mut svm, &s, &principal, &principal_ta, &merchant_ata, 1),
        "root spending after redeeming its free budget",
        E_HOOK_CAP_EXCEEDED,
    );

    // The agent's delegated budget survived all of it, and is still spendable.
    expect_ok(spend(&mut svm, &s, &agent, &token_account_pda(&agent.pubkey(), 0), &merchant_ata, 400));
    assert_eq!(token_amount(&svm, &merchant_ata), 500);
}
