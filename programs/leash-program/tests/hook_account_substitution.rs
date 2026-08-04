//! Does leash-hook trust the accounts it is handed?
//!
//! `spend_logic` reads its ancestors positionally — `accounts[7]`, `[8]`, `[9]` — and
//! checks `!ancestor.revoked` on each. Unlike `accounts[6]` (the source capability, which
//! is pinned by `accounts[0].key() == capability.token_account`), **nothing in the hook
//! binds those ancestor slots to anything**. The hook does not call
//! `ExtraAccountMetaList::check_account_infos`, which is the interface's own mechanism for
//! verifying that the accounts a caller supplied are the ones the registered formula
//! resolves to.
//!
//! So the entire revocation guarantee for delegated capabilities rested on an assumption
//! nowhere stated in this repo: that **Token-2022 itself** validates the extra accounts
//! before invoking the hook. If it merely forwarded them, a revoked parent would stop
//! nothing — swap in any unrevoked capability as "ancestor1" and the walk passes. That is
//! property 4 (BUILD_PLAN.md §2) defeated for every delegated capability, which is most of
//! what Leash sells.
//!
//! **The assumption holds, and this file is what turned it from an assumption into a
//! tested fact.** Token-2022 calls `ExtraAccountMetaList::check_account_infos` before
//! invoking the hook, so a substituted account is rejected upstream with
//! `AccountResolutionError::IncorrectAccount` and `spend_logic` never runs. The hook is
//! therefore correct to read its ancestors positionally.
//!
//! Kept as a regression test rather than deleted as a false alarm, because the property is
//! **inherited from a dependency, not enforced here**. Nothing in this repo would notice
//! if a future Token-2022 relaxed that validation, and nothing in this repo would notice
//! if leash-hook's registered account layout drifted out of step with what `spend_logic`
//! indexes. This test notices both. It is deliberately asserted against the specific
//! upstream error code rather than `is_err()` (docs/ROADMAP.md 0.5): a substituted spend
//! that failed for some *other* reason — a stale blockhash, a balance check — would
//! otherwise look like proof of a guarantee that had quietly stopped holding.

mod common;

use common::*;

use anchor_spl::token_2022::spl_token_2022;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;

use anchor_lang::prelude::Pubkey;

const FAR_FUTURE: i64 = 4_102_444_800; // 2100-01-01

/// A spend built exactly like `common::spend`, except that after the extra accounts are
/// resolved, every occurrence of `swap_from` in the account list is rewritten to
/// `swap_to`. That is the whole experiment: identical transaction, one account changed.
#[allow(clippy::too_many_arguments)]
fn spend_with_substituted_account(
    svm: &mut LiteSVM,
    s: &Setup,
    source_owner: &Keypair,
    source: &Pubkey,
    destination: &Pubkey,
    amount: u64,
    swap_from: &Pubkey,
    swap_to: &Pubkey,
) -> Result<(), String> {
    let mut transfer_ix = spl_token_2022::instruction::transfer_checked(
        &spl_token_2022::id(),
        source,
        &s.wrapped_mint,
        destination,
        &source_owner.pubkey(),
        &[],
        amount,
        6,
    )
    .unwrap();

    futures::executor::block_on(
        spl_transfer_hook_interface::offchain::add_extra_account_metas_for_execute(
            &mut transfer_ix,
            &HOOK_ID,
            source,
            &s.wrapped_mint,
            destination,
            &source_owner.pubkey(),
            amount,
            |pubkey| {
                let account = svm.get_account(&pubkey);
                async move { Ok(account.map(|a| a.data)) }
            },
        ),
    )
    .map_err(|e| format!("extra-account resolution failed: {:?}", e))?;

    let mut swapped = 0usize;
    for meta in transfer_ix.accounts.iter_mut() {
        if meta.pubkey == *swap_from {
            meta.pubkey = *swap_to;
            swapped += 1;
        }
    }
    assert!(
        swapped > 0,
        "substitution target {} was not in the resolved account list — the test is not \
         exercising what it claims to",
        swap_from
    );

    send(svm, &s.payer, &[source_owner], &[transfer_ix])
}

/// Substituting a live capability into the ancestor slot of a revoked parent must not let
/// the spend through.
#[test]
fn a_revoked_ancestor_cannot_be_swapped_out_for_a_live_one() {
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

    // Victim principal: a root capability that will be revoked, and an agent under it.
    let principal = Keypair::new();
    let agent = Keypair::new();
    expect_ok(issue(
        &mut svm,
        &s,
        &principal,
        0,
        1_000,
        FAR_FUTURE,
        vec![merchant_ata],
    ));
    let (root_capability, _) = capability_for(&principal.pubkey(), 0);
    expect_ok(attenuate(
        &mut svm,
        &s,
        &principal,
        root_capability,
        &agent,
        1,
        500,
        FAR_FUTURE,
        vec![merchant_ata],
    ));
    let (_, agent_tokens) = capability_for(&agent.pubkey(), 1);

    // An unrelated, perfectly live capability to stand in as a forged ancestor.
    let bystander = Keypair::new();
    expect_ok(issue(
        &mut svm,
        &s,
        &bystander,
        0,
        1_000,
        FAR_FUTURE,
        vec![merchant_ata],
    ));
    let (bystander_capability, _) = capability_for(&bystander.pubkey(), 0);

    // The principal cuts the agent off.
    expect_ok(revoke(&mut svm, &s, &principal, root_capability));

    // Sanity: the honest path is genuinely blocked, and for the right reason. Without
    // this, a substituted spend that also fails would prove nothing.
    expect_err_code(
        spend(&mut svm, &s, &agent, &agent_tokens, &merchant_ata, 100),
        "spending under a revoked parent",
        E_HOOK_PARENT_REVOKED,
    );

    // THE ATTEMPT: same transfer, with the revoked parent swapped out of the ancestor
    // slot for a live capability.
    let res = spend_with_substituted_account(
        &mut svm,
        &s,
        &agent,
        &agent_tokens,
        &merchant_ata,
        100,
        &root_capability,
        &bystander_capability,
    );

    expect_err_code(
        res,
        "substituting a live capability into a revoked parent's ancestor slot",
        E_RESOLUTION_INCORRECT_ACCOUNT,
    );

    assert_eq!(
        token_amount(&svm, &merchant_ata),
        0,
        "no units may reach the merchant from a revoked tree"
    );
}
