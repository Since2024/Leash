//! Proves the vault and the wrapped mint are actually *bound to each other*, so neither
//! `issue` nor `redeem` can be pointed at a substitute (docs/ROADMAP.md 0.11).
//!
//! Both instructions used to take `vault` — and `redeem` also `wrapped_mint` — as
//! completely unconstrained `UncheckedAccount`s. Nothing on-chain said which vault was
//! *the* vault, because nothing on-chain ever recorded one: `createDeployment`
//! (`sdk/ts/src/deployment.ts`) generates the vault as a random keypair account, and the
//! only thing tying it to the program is that its token-account authority is the shared
//! `program_authority` PDA. The caller picks the rest.
//!
//! The single check that *was* present — the `seeds` constraint on `program_authority` —
//! looks like it closes this and does not. It proves the signer is the canonical
//! authority PDA; it says nothing about which token account that PDA is being made to
//! sign a withdrawal *from*. And because `program_authority` is seeded `[AUTHORITY_SEED]`
//! with no mint or deployment in the seeds, one PDA is the authority for every vault the
//! program will ever have, so "authority matches" is satisfied by the real vault no
//! matter what mint the caller burns.
//!
//! Two separate ways that cashed out, both tested below and both draining the *real*
//! vault of a *third party's* deposit:
//!
//! 1. `redeem` with a counterfeit wrapped mint. The burn is of the caller's own worthless
//!    Token-2022 mint; the payout is from the real vault. One instruction, no capability,
//!    no deposit, no prior state.
//! 2. `issue` with a substitute vault. The deposit lands in an account the caller owns
//!    while the *real* wrapped mint still mints them a fully-backed-looking capability —
//!    which then redeems against the real vault by the ordinary, entirely legitimate path.
//!
//! Neither needs a forged account, an unusual signer, or a race. Both are plain calls
//! with one account swapped, which is why every existing test misses them: the helpers in
//! `common/mod.rs` always pass `s.vault` and `s.wrapped_mint`, so the suite only ever
//! exercised the honest wiring.
//!
//! The fix binds the vault to the wrapped mint by derivation — the vault is a PDA at
//! `[VAULT_SEED, wrapped_mint]` — so "which vault" is answered by the same seeds that
//! answer "which mint", and a mismatched pair fails Anchor's own `seeds` check before any
//! handler code runs. Derivation rather than a stored field is deliberate: a recorded
//! `vault` pubkey would be one more thing written once and trusted forever, which is
//! exactly how docs/ROADMAP.md 0.4 went wrong.

mod common;

use common::*;

use anchor_lang::solana_program::program_pack::Pack;
use anchor_lang::{InstructionData, ToAccountMetas};
use anchor_spl::token::spl_token;
use anchor_spl::token_2022::spl_token_2022;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;

use anchor_lang::prelude::Pubkey;

const FAR_FUTURE: i64 = 4_102_444_800; // 2100-01-01

/// Anchor's built-in `ErrorCode::ConstraintSeeds`. The mismatch is caught by the account
/// constraint itself, before any handler runs, so there is no `LeashError` to assert —
/// this is the code that actually surfaces, and asserting the real one is the point
/// (docs/ROADMAP.md 0.5).
const E_ANCHOR_CONSTRAINT_SEEDS: u32 = 2006;

/// A vault is a PDA of the wrapped mint it backs. That derivation *is* the binding — see
/// the module doc.
fn vault_pda(wrapped_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"vault", wrapped_mint.as_ref()], &PROGRAM_ID).0
}

/// A plain Token-2022 mint the attacker controls outright: no transfer hook, no
/// relationship to the deployment, mint authority theirs. Standing in for "any token the
/// caller can create," which is the whole difficulty — a mint is permissionless.
fn attacker_mint(svm: &mut LiteSVM, s: &Setup, authority: &Pubkey) -> Pubkey {
    let mint = Keypair::new();
    let space = spl_token_2022::state::Mint::LEN;
    let rent = svm.minimum_balance_for_rent_exemption(space);
    send(
        svm,
        &s.payer,
        &[&mint],
        &[
            anchor_lang::solana_program::system_instruction::create_account(
                &s.payer.pubkey(),
                &mint.pubkey(),
                rent,
                space as u64,
                &spl_token_2022::id(),
            ),
            spl_token_2022::instruction::initialize_mint2(
                &spl_token_2022::id(),
                &mint.pubkey(),
                authority,
                None,
                6,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    mint.pubkey()
}

/// Creates `who`'s ATA for `mint` and mints `amount` into it, signed by `mint_authority`.
fn fund_token_account(
    svm: &mut LiteSVM,
    s: &Setup,
    who: &Pubkey,
    mint: &Pubkey,
    mint_authority: &Keypair,
    amount: u64,
) -> Pubkey {
    let acct = ata(who, mint, &spl_token_2022::id());
    send(
        svm,
        &s.payer,
        &[],
        &[create_ata_ix(
            &s.payer.pubkey(),
            who,
            mint,
            &spl_token_2022::id(),
        )],
    )
    .unwrap();
    send(
        svm,
        &s.payer,
        &[mint_authority],
        &[spl_token_2022::instruction::mint_to(
            &spl_token_2022::id(),
            mint,
            &acct,
            &mint_authority.pubkey(),
            &[],
            amount,
        )
        .unwrap()],
    )
    .unwrap();
    acct
}

/// Gives `who` a legacy-SPL USDC account to receive a payout into.
fn usdc_account(svm: &mut LiteSVM, s: &Setup, who: &Pubkey) -> Pubkey {
    let acct = ata(who, &s.usdc_mint, &spl_token::id());
    send(
        svm,
        &s.payer,
        &[],
        &[create_ata_ix(
            &s.payer.pubkey(),
            who,
            &s.usdc_mint,
            &spl_token::id(),
        )],
    )
    .unwrap();
    acct
}

/// `redeem` with every account under the caller's control. `common::redeem` hardcodes
/// `s.wrapped_mint` and `s.vault`, which is precisely why it could never have caught
/// this — the helper was the thing enforcing the binding, not the program.
#[allow(clippy::too_many_arguments)]
fn redeem_with(
    svm: &mut LiteSVM,
    s: &Setup,
    holder: &Keypair,
    holder_wrapped_account: Pubkey,
    wrapped_mint: Pubkey,
    vault: Pubkey,
    holder_deposit_account: Pubkey,
    amount: u64,
) -> Result<(), String> {
    let ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::Redeem {
            holder: holder.pubkey(),
            holder_wrapped_account,
            capability: capability_pda(&holder_wrapped_account),
            wrapped_mint,
            vault,
            program_authority: s.program_authority,
            holder_deposit_account,
            token_program: spl_token::id(),
            token_2022_program: spl_token_2022::id(),
        }
        .to_account_metas(None),
        data: leash_program::instruction::Redeem { amount }.data(),
    };
    send(svm, &s.payer, &[holder], &[ix])
}

/// `issue` with a caller-chosen vault, everything else honest.
#[allow(clippy::too_many_arguments)]
fn issue_with_vault(
    svm: &mut LiteSVM,
    s: &Setup,
    principal: &Keypair,
    principal_deposit_account: Pubkey,
    vault: Pubkey,
    nonce: u64,
    cap: u64,
) -> Result<(), String> {
    let (capability, capability_token_account) = capability_for(&principal.pubkey(), nonce);
    let ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::Issue {
            principal: principal.pubkey(),
            principal_deposit_account,
            vault,
            wrapped_mint: s.wrapped_mint,
            program_authority: s.program_authority,
            capability_token_account,
            capability,
            token_program: spl_token::id(),
            token_2022_program: spl_token_2022::id(),
            system_program: anchor_lang::solana_program::system_program::ID,
        }
        .to_account_metas(None),
        data: leash_program::instruction::Issue {
            nonce,
            cap,
            expiry: FAR_FUTURE,
            allowlist: vec![],
        }
        .data(),
    };
    send(svm, &s.payer, &[principal], &[ix])
}

/// THE HOLE, shortest form. The attacker never deposits, never holds a capability, and
/// never touches the real wrapped mint. They burn a token they minted themselves five
/// seconds earlier and walk off with somebody else's deposit.
///
/// Pre-fix this succeeded outright: `wrapped_mint` was an `UncheckedAccount` used only as
/// the burn's mint, and `vault` an `UncheckedAccount` used only as the withdrawal's
/// source, with nothing anywhere requiring the two to belong to the same deployment.
#[test]
fn redeem_rejects_a_counterfeit_wrapped_mint() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);

    // A legitimate principal deposits 1_000 real USDC. This is the money at risk; it has
    // nothing to do with the attacker and the attacker has no claim on any of it.
    let principal = Keypair::new();
    expect_ok(issue(&mut svm, &s, &principal, 0, 1_000, FAR_FUTURE, vec![]));
    assert_eq!(token_amount(&svm, &s.vault), 1_000, "vault funded");

    // The attacker's own worthless Token-2022 mint, and 1_000 units of it.
    let attacker = Keypair::new();
    svm.airdrop(&attacker.pubkey(), 5_000_000_000).unwrap();
    let counterfeit = attacker_mint(&mut svm, &s, &attacker.pubkey());
    let counterfeit_account =
        fund_token_account(&mut svm, &s, &attacker.pubkey(), &counterfeit, &attacker, 1_000);
    let attacker_usdc = usdc_account(&mut svm, &s, &attacker.pubkey());

    // Burn the counterfeit, get paid from the real vault.
    let res = redeem_with(
        &mut svm,
        &s,
        &attacker,
        counterfeit_account,
        counterfeit,
        s.vault, // the REAL vault, holding the principal's 1_000
        attacker_usdc,
        1_000,
    );
    expect_err_code(
        res,
        "redeeming a counterfeit mint against the real vault",
        E_ANCHOR_CONSTRAINT_SEEDS,
    );

    // The assertion that actually says "no money moved" — an error code alone would not.
    assert_eq!(
        token_amount(&svm, &s.vault),
        1_000,
        "the real vault must be untouched by a redemption of some other mint"
    );
    assert_eq!(
        token_amount(&svm, &attacker_usdc),
        0,
        "the attacker must not have received real USDC"
    );
}

/// The same binding, from the other side. Here the attacker uses the *real* wrapped mint
/// — so the units they receive are genuine, hook-enforced capability budget — but sends
/// the deposit that is supposed to back them into an account they own.
///
/// Pre-fix, `issue` transferred to whatever `vault` it was handed and then minted against
/// the real mint regardless, so this produced a capability indistinguishable from an
/// honestly-funded one. Redeeming it afterwards needs no trickery at all: it is the
/// ordinary root-redemption path, drawing on collateral somebody else posted.
#[test]
fn issue_rejects_a_substitute_vault() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);

    let principal = Keypair::new();
    expect_ok(issue(&mut svm, &s, &principal, 0, 1_000, FAR_FUTURE, vec![]));
    assert_eq!(token_amount(&svm, &s.vault), 1_000, "vault funded");

    // A decoy vault: an ordinary USDC token account the attacker controls. Note it does
    // not even need `program_authority` as its authority — nothing was checking — but
    // making it authority-owned would not help the program either, since anyone may
    // create a token account owned by any pubkey.
    let attacker = Keypair::new();
    svm.airdrop(&attacker.pubkey(), 5_000_000_000).unwrap();
    let decoy_vault = Keypair::new();
    let vault_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Account::LEN);
    send(
        &mut svm,
        &s.payer,
        &[&decoy_vault],
        &[
            anchor_lang::solana_program::system_instruction::create_account(
                &s.payer.pubkey(),
                &decoy_vault.pubkey(),
                vault_rent,
                spl_token::state::Account::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_account3(
                &spl_token::id(),
                &decoy_vault.pubkey(),
                &s.usdc_mint,
                &s.program_authority,
            )
            .unwrap(),
        ],
    )
    .unwrap();

    // Fund the attacker so the deposit leg can genuinely succeed — the point is that the
    // money goes somewhere it should not, not that the transfer fails.
    let attacker_usdc = usdc_account(&mut svm, &s, &attacker.pubkey());
    send(
        &mut svm,
        &s.payer,
        &[&s.usdc_mint_authority],
        &[spl_token::instruction::mint_to(
            &spl_token::id(),
            &s.usdc_mint,
            &attacker_usdc,
            &s.usdc_mint_authority.pubkey(),
            &[],
            1_000,
        )
        .unwrap()],
    )
    .unwrap();

    let res = issue_with_vault(
        &mut svm,
        &s,
        &attacker,
        attacker_usdc,
        decoy_vault.pubkey(),
        7,
        1_000,
    );
    expect_err_code(res, "issuing against a substitute vault", E_ANCHOR_CONSTRAINT_SEEDS);

    assert_eq!(
        token_amount(&svm, &decoy_vault.pubkey()),
        0,
        "no deposit may land anywhere but the deployment's own vault"
    );
    let (_, attacker_token_account) = capability_for(&attacker.pubkey(), 7);
    assert!(
        svm.get_account(&attacker_token_account)
            .map_or(true, |a| a.data.is_empty()),
        "no wrapped units may be minted when the deposit did not reach the vault"
    );
}

/// The honest path still works, against a vault that is now a PDA of the wrapped mint.
/// Worth asserting explicitly: a binding that also broke ordinary deposits and
/// redemptions would be a regression dressed as a fix.
#[test]
fn the_bound_vault_still_supports_an_ordinary_deposit_and_redemption() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);

    assert_eq!(
        s.vault,
        vault_pda(&s.wrapped_mint),
        "setup must be using the derived vault, or this file proves nothing"
    );

    let principal = Keypair::new();
    expect_ok(issue(&mut svm, &s, &principal, 0, 1_000, FAR_FUTURE, vec![]));
    assert_eq!(token_amount(&svm, &s.vault), 1_000);

    let principal_usdc = ata(&principal.pubkey(), &s.usdc_mint, &spl_token::id());
    let (_, principal_token_account) = capability_for(&principal.pubkey(), 0);
    expect_ok(redeem(
        &mut svm,
        &s,
        &principal,
        principal_token_account,
        principal_usdc,
        400,
    ));

    assert_eq!(token_amount(&svm, &s.vault), 600, "vault paid out 400");
    assert_eq!(token_amount(&svm, &principal_usdc), 400);
}
