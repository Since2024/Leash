//! Property 6 (BUILD_PLAN.md §2): a spend must fail **closed** when the enforcing program
//! cannot run. Five of the six non-negotiable properties had real tests; this one was
//! argued from how Token-2022 works and never exercised — the same shape as the invariant
//! in docs/ROADMAP.md 0.2, which was promised in writing at the place a reader would check
//! and enforced nowhere.
//!
//! # Getting the account list right is most of the work here
//!
//! A hooked `transfer_checked` carries, after its four base accounts:
//! `[...resolved extras, hook_program, extra_account_meta_list]`. A first draft of this
//! file appended **none** of that and asserted only that the transfer failed. All four
//! tests passed, and all four were worthless: every one died with `MissingAccount` because
//! the hook program was not in the account list at all, so none of them reached the
//! condition they were named after. A test asserting "it failed" against an account list
//! that could never have succeeded proves nothing — exactly the trap docs/ROADMAP.md's
//! verification note describes, and it caught this file on the first attempt.
//!
//! So each test below supplies everything Token-2022 needs *except* the one thing under
//! test, and the module keeps two controls: an unhooked mint that transfers fine, and (in
//! the bypass test) the identical spend with its accounts correctly resolved, succeeding.

mod common;

use common::*;

use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_spl::token_2022::spl_token_2022;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;

use anchor_lang::prelude::Pubkey;

const FAR_FUTURE: i64 = 4_102_444_800; // 2100-01-01

/// The Transfer Hook Interface's meta-list PDA for a mint.
fn meta_list_pda(mint: &Pubkey, hook: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"extra-account-metas", mint.as_ref()], hook).0
}

/// Builds a Token-2022 mint, optionally with a TransferHook pointing at `hook`, plus two
/// funded accounts to move units between. Returns `(mint, source, destination)`.
fn mint_with_optional_hook(
    svm: &mut LiteSVM,
    s: &Setup,
    holder: &Keypair,
    hook: Option<Pubkey>,
    amount: u64,
) -> (Pubkey, Pubkey, Pubkey) {
    let mint = Keypair::new();
    let extensions: &[spl_token_2022::extension::ExtensionType] = match hook {
        Some(_) => &[spl_token_2022::extension::ExtensionType::TransferHook],
        None => &[],
    };
    let space = spl_token_2022::extension::ExtensionType::try_calculate_account_len::<
        spl_token_2022::state::Mint,
    >(extensions)
    .unwrap();
    let rent = svm.minimum_balance_for_rent_exemption(space);

    let mut ixs = vec![anchor_lang::solana_program::system_instruction::create_account(
        &s.payer.pubkey(),
        &mint.pubkey(),
        rent,
        space as u64,
        &spl_token_2022::id(),
    )];
    if let Some(hook_program) = hook {
        ixs.push(
            spl_token_2022::extension::transfer_hook::instruction::initialize(
                &spl_token_2022::id(),
                &mint.pubkey(),
                Some(s.program_authority),
                Some(hook_program),
            )
            .unwrap(),
        );
    }
    ixs.push(
        spl_token_2022::instruction::initialize_mint2(
            &spl_token_2022::id(),
            &mint.pubkey(),
            &holder.pubkey(),
            None,
            6,
        )
        .unwrap(),
    );
    send(svm, &s.payer, &[&mint], &ixs).unwrap();

    let source = ata(&holder.pubkey(), &mint.pubkey(), &spl_token_2022::id());
    let dest_owner = Keypair::new();
    let destination = ata(&dest_owner.pubkey(), &mint.pubkey(), &spl_token_2022::id());
    send(
        svm,
        &s.payer,
        &[],
        &[
            create_ata_ix(
                &s.payer.pubkey(),
                &holder.pubkey(),
                &mint.pubkey(),
                &spl_token_2022::id(),
            ),
            create_ata_ix(
                &s.payer.pubkey(),
                &dest_owner.pubkey(),
                &mint.pubkey(),
                &spl_token_2022::id(),
            ),
        ],
    )
    .unwrap();

    // Minting fires no transfer hook, so this works whatever state the hook is in — worth
    // knowing in its own right: only the *transfer* path is guarded.
    send(
        svm,
        &s.payer,
        &[holder],
        &[spl_token_2022::instruction::mint_to(
            &spl_token_2022::id(),
            &mint.pubkey(),
            &source,
            &holder.pubkey(),
            &[],
            amount,
        )
        .unwrap()],
    )
    .unwrap();

    (mint.pubkey(), source, destination)
}

/// `transfer_checked` with an explicit set of trailing accounts, so each test controls
/// exactly how much of the hook's machinery is present.
#[allow(clippy::too_many_arguments)]
fn transfer_with_trailing(
    svm: &mut LiteSVM,
    s: &Setup,
    holder: &Keypair,
    mint: &Pubkey,
    source: &Pubkey,
    destination: &Pubkey,
    amount: u64,
    trailing: &[AccountMeta],
) -> Result<(), String> {
    let mut ix = spl_token_2022::instruction::transfer_checked(
        &spl_token_2022::id(),
        source,
        mint,
        destination,
        &holder.pubkey(),
        &[],
        amount,
        6,
    )
    .unwrap();
    ix.accounts.extend_from_slice(trailing);
    send(svm, &s.payer, &[holder], &[ix])
}

/// **Control.** An unhooked Token-2022 mint moves units fine by this exact code path.
#[test]
fn control_a_mint_with_no_hook_transfers_normally() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let holder = Keypair::new();
    svm.airdrop(&holder.pubkey(), 5_000_000_000).unwrap();

    let (mint, source, destination) = mint_with_optional_hook(&mut svm, &s, &holder, None, 1_000);
    expect_ok(transfer_with_trailing(
        &mut svm,
        &s,
        &holder,
        &mint,
        &source,
        &destination,
        400,
        &[],
    ));
    assert_eq!(token_amount(&svm, &destination), 400);
}

/// The headline case: the hook program named by the mint **is not deployed**, and is
/// supplied in the account list anyway. If Token-2022 skipped a hook it could not invoke,
/// every guarantee in this project would be void — enforcement is wholly delegated to that
/// program.
#[test]
fn a_transfer_fails_when_the_hook_program_does_not_exist() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let holder = Keypair::new();
    svm.airdrop(&holder.pubkey(), 5_000_000_000).unwrap();

    let absent_hook = Pubkey::new_unique();
    assert!(
        svm.get_account(&absent_hook)
            .map_or(true, |a| a.data.is_empty()),
        "the point of this test is that nothing is deployed here"
    );

    let (mint, source, destination) =
        mint_with_optional_hook(&mut svm, &s, &holder, Some(absent_hook), 1_000);

    // Hand Token-2022 the hook program and meta list it would look for, so the failure is
    // about the program not existing rather than about the account list being short.
    let trailing = [
        AccountMeta::new_readonly(absent_hook, false),
        AccountMeta::new_readonly(meta_list_pda(&mint, &absent_hook), false),
    ];
    let res = transfer_with_trailing(
        &mut svm,
        &s,
        &holder,
        &mint,
        &source,
        &destination,
        400,
        &trailing,
    );
    // PROPERTY 6: had this succeeded, Token-2022 would be skipping hooks it cannot
    // invoke, and every guarantee in this project would be void.
    expect_err_matching(
        res,
        "a transfer whose hook program does not exist",
        "InvalidAccountData",
    );
    assert_eq!(
        token_amount(&svm, &destination),
        0,
        "no units may move when the enforcing program cannot run"
    );
}

/// The hook program is real and deployed; the mint's `ExtraAccountMetaList` was never
/// registered. This is the state a deployment sits in between creating its mint and
/// calling `initialize_extra_account_meta_list` — which `createDeployment` now does in one
/// transaction precisely so the window does not exist.
#[test]
fn a_transfer_fails_when_the_meta_list_was_never_registered() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let holder = Keypair::new();
    svm.airdrop(&holder.pubkey(), 5_000_000_000).unwrap();

    let (mint, source, destination) =
        mint_with_optional_hook(&mut svm, &s, &holder, Some(HOOK_ID), 1_000);
    let meta_list = meta_list_pda(&mint, &HOOK_ID);
    assert!(
        svm.get_account(&meta_list).map_or(true, |a| a.data.is_empty()),
        "no meta list should exist for this mint"
    );

    let trailing = [
        AccountMeta::new_readonly(HOOK_ID, false),
        AccountMeta::new_readonly(meta_list, false),
    ];
    let res = transfer_with_trailing(
        &mut svm,
        &s,
        &holder,
        &mint,
        &source,
        &destination,
        400,
        &trailing,
    );
    expect_err_matching(
        res,
        "a transfer with no ExtraAccountMetaList registered",
        "InvalidAccountData",
    );
    assert_eq!(token_amount(&svm, &destination), 0);
}

/// The meta list exists but its contents are truncated — corruption rather than absence.
/// Written directly into the account, which only a test can do: on a real chain that
/// address is a PDA of leash-hook and only leash-hook can write it. That is the point —
/// this establishes what happens *if* that ever stopped being true.
#[test]
fn a_transfer_fails_when_the_meta_list_is_corrupt() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let holder = Keypair::new();
    svm.airdrop(&holder.pubkey(), 5_000_000_000).unwrap();

    let (mint, source, destination) =
        mint_with_optional_hook(&mut svm, &s, &holder, Some(HOOK_ID), 1_000);
    let meta_list = meta_list_pda(&mint, &HOOK_ID);

    // Clone the *real* deployment's meta list and truncate it. Starting from a genuine
    // account keeps this honest: the result is owned by leash-hook and rent-funded, correct
    // in every respect except that its TLV data has been cut short — so the rejection is
    // attributable to the corruption and not to some unrelated difference.
    let real_meta_list = meta_list_pda(&s.wrapped_mint, &HOOK_ID);
    let mut corrupt = svm.get_account(&real_meta_list).unwrap();
    assert!(
        !corrupt.data.is_empty() && corrupt.owner == HOOK_ID,
        "template must be a real, populated meta list"
    );
    corrupt.data.truncate(16);
    svm.set_account(meta_list, corrupt).unwrap();

    let trailing = [
        AccountMeta::new_readonly(HOOK_ID, false),
        AccountMeta::new_readonly(meta_list, false),
    ];
    let res = transfer_with_trailing(
        &mut svm,
        &s,
        &holder,
        &mint,
        &source,
        &destination,
        400,
        &trailing,
    );
    expect_err_matching(
        res,
        "a transfer against a corrupt ExtraAccountMetaList",
        "InvalidAccountData",
    );
    assert_eq!(token_amount(&svm, &destination), 0);
}

/// The bypass an attacker would actually try on the **real** deployment. leash-hook's
/// enforcement lives entirely in accounts appended to the transfer, so the obvious move is
/// to supply the hook program and its meta list — enough that Token-2022 is willing to
/// proceed — while withholding the resolved accounts the checks are read from.
///
/// The control below is decisive: the identical spend with its accounts resolved properly
/// succeeds, so the capability, balance and allowlist are all fine and the *only*
/// difference is the withheld accounts.
#[test]
fn withholding_the_hooks_resolved_accounts_cannot_bypass_enforcement() {
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
    let (capability, token_account) = capability_for(&principal.pubkey(), 0);

    // Everything Token-2022 needs to dispatch into leash-hook, and nothing it needs to
    // actually check anything.
    let trailing = [
        AccountMeta::new_readonly(HOOK_ID, false),
        AccountMeta::new_readonly(meta_list_pda(&s.wrapped_mint, &HOOK_ID), false),
    ];
    let res = transfer_with_trailing(
        &mut svm,
        &s,
        &principal,
        &s.wrapped_mint,
        &token_account,
        &merchant_ata,
        400,
        &trailing,
    );
    // Caught by the same upstream mechanism as the substitution attack in
    // `hook_account_substitution.rs`: Token-2022 validates the supplied accounts against
    // the registered formula before dispatching, so a short list never reaches the hook.
    expect_err_code(
        res,
        "a spend withholding leash-hook's resolved accounts",
        E_RESOLUTION_INCORRECT_ACCOUNT,
    );
    assert_eq!(
        token_amount(&svm, &merchant_ata),
        0,
        "no units may move without the hook's checks running"
    );
    assert_eq!(
        capability_state(&svm, &capability).spent,
        0,
        "and nothing may be recorded as spent"
    );

    // Control: the identical spend, resolved properly, succeeds.
    expect_ok(spend(
        &mut svm,
        &s,
        &principal,
        &token_account,
        &merchant_ata,
        400,
    ));
    assert_eq!(token_amount(&svm, &merchant_ata), 400);
    assert_eq!(capability_state(&svm, &capability).spent, 400);
}

/// A `Capability` account whose bytes are garbage. leash-hook deserializes the source
/// capability with a bare `AnchorDeserialize` over `data[8..]` and **never checks the
/// discriminator**, so this pins down what happens when those bytes are not what it
/// expects.
///
/// Unreachable on a real chain today — that address is a PDA of leash-program, and only
/// leash-program can write accounts it owns, so nothing but a genuine `Capability` can be
/// there. It is fabricated here with `set_account` precisely because being unreachable is
/// a property of the *current* instruction set rather than of the hook, and the hook
/// should not depend on it. What must not happen is enforcement being skipped.
#[test]
fn a_transfer_fails_when_the_capability_is_unreadable() {
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
    let (capability, token_account) = capability_for(&principal.pubkey(), 0);

    // Control first: this exact spend works while the capability is intact, so the failure
    // below is attributable to the corruption and nothing else.
    expect_ok(spend(
        &mut svm,
        &s,
        &principal,
        &token_account,
        &merchant_ata,
        100,
    ));
    assert_eq!(token_amount(&svm, &merchant_ata), 100);

    // Now scribble over it, keeping owner and length intact so it still *looks* like a
    // capability to everything except a parse.
    let mut wrecked = svm.get_account(&capability).unwrap();
    assert_eq!(wrecked.owner, PROGRAM_ID);
    let len = wrecked.data.len();
    wrecked.data = vec![0xFF; len];
    svm.set_account(capability, wrecked).unwrap();

    let res = spend(&mut svm, &s, &principal, &token_account, &merchant_ata, 100);
    // `InvalidAccountData` is leash-hook's own `map_err` on the failed deserialize —
    // i.e. the hook noticed and refused, rather than proceeding on garbage values.
    expect_err_matching(
        res,
        "a spend against an unreadable Capability",
        "InvalidAccountData",
    );
    assert_eq!(
        token_amount(&svm, &merchant_ata),
        100,
        "the merchant balance must be unchanged by the failed spend"
    );
}
