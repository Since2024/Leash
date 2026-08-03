//! Proves a capability can only ever mint and spend units of the mint it was issued
//! against (docs/ROADMAP.md 0.12).
//!
//! `attenuate` mints the child's units with `program_authority` as the signing mint
//! authority, and that PDA is the mint authority for the real wrapped mint. But nothing
//! tied the `wrapped_mint` passed to `attenuate` to the mint the *parent capability* was
//! issued against — `Capability` did not record its mint, and `TOKEN_ACCOUNT_SEED` does
//! not contain one either, so there was no field and no derivation to check it with.
//!
//! The consequence is a total loss of the vault, backed by nothing:
//!
//! 1. Anyone may create a Token-2022 mint naming an arbitrary pubkey as its mint
//!    authority — setting an authority needs no signature from it. So the attacker
//!    creates a second "wrapped mint" whose mint authority is leash's own
//!    `program_authority`, plus a worthless deposit asset to back it.
//! 2. `initialize_vault` for that pair, then `issue` against it, depositing the worthless
//!    asset. This is entirely legitimate: it is the attacker's own self-consistent
//!    deployment, and docs/ROADMAP.md 0.11 explicitly permits it as harmless.
//! 3. `attenuate` from that capability while naming the **real** wrapped mint. The mint
//!    authority still checks out, because it is the same PDA for every deployment — so
//!    the child receives genuine units of the real mint, with a genuine capability,
//!    honoured by the hook like any other.
//! 4. Spend them to an address on the attacker's own allowlist, and redeem. The units
//!    arrive at the destination through a real hook-checked transfer, so they redeem by
//!    the ordinary merchant path.
//!
//! Step 3 is the only illegitimate one, and it is a single unchecked account.
//!
//! This is the same shape as 0.11 one level in: 0.11 pinned *which vault* a mint is paid
//! out of, and this pins *which mint* a capability may mint. Both were accounts the
//! program used without ever asking whether they belonged together, and in both the
//! `program_authority` seeds check looks like it covers the gap and does not — it proves
//! the signer is canonical, never what it is being made to sign for.
//!
//! Fixed by recording `wrapped_mint` on `Capability` and requiring `attenuate`'s mint to
//! equal the parent's. The field is placed after `token_account` deliberately: leash-hook
//! reads `parent` and `ancestors` out of raw account bytes at fixed offsets
//! (`PARENT_FIELD_OFFSET` / `ANCESTORS_FIELD_OFFSET`), so a field inserted before them
//! would silently repoint the hook's ancestor resolution at garbage.

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

/// `LeashError::WrongMint` — asserted specifically rather than as a bare failure, because
/// a mint mismatch and a Token-2022 account/mint mismatch are indistinguishable to
/// `is_err()` and only one of them means the program did its job (docs/ROADMAP.md 0.5).
const E_LEASH_WRONG_MINT: u32 = 6013;

fn vault_pda(wrapped_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"vault", wrapped_mint.as_ref()], &PROGRAM_ID).0
}

/// A Token-2022 mint whose mint authority is leash's `program_authority`. The whole point
/// is that this requires no cooperation from that PDA: `initialize_mint2` takes the
/// authority as a plain argument, and nobody has to sign for being named one.
fn mint_with_leash_authority(svm: &mut LiteSVM, s: &Setup) -> Pubkey {
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
                &s.program_authority,
                None,
                6,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    mint.pubkey()
}

/// A worthless legacy-SPL asset the attacker controls, standing in for "not USDC".
fn worthless_asset(svm: &mut LiteSVM, s: &Setup, authority: &Keypair) -> Pubkey {
    let mint = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
    send(
        svm,
        &s.payer,
        &[&mint],
        &[
            anchor_lang::solana_program::system_instruction::create_account(
                &s.payer.pubkey(),
                &mint.pubkey(),
                rent,
                spl_token::state::Mint::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_mint2(
                &spl_token::id(),
                &mint.pubkey(),
                &authority.pubkey(),
                None,
                6,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    mint.pubkey()
}

fn init_vault(svm: &mut LiteSVM, s: &Setup, wrapped_mint: &Pubkey, deposit_mint: &Pubkey) -> Pubkey {
    let vault = vault_pda(wrapped_mint);
    let ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::InitializeVault {
            payer: s.payer.pubkey(),
            wrapped_mint: *wrapped_mint,
            deposit_mint: *deposit_mint,
            program_authority: s.program_authority,
            vault,
            token_program: spl_token::id(),
            system_program: anchor_lang::solana_program::system_program::ID,
        }
        .to_account_metas(None),
        data: leash_program::instruction::InitializeVault {}.data(),
    };
    send(svm, &s.payer, &[], &[ix]).unwrap();
    vault
}

/// `issue` against an arbitrary (wrapped_mint, vault) pair rather than `s`'s.
#[allow(clippy::too_many_arguments)]
fn issue_against(
    svm: &mut LiteSVM,
    s: &Setup,
    principal: &Keypair,
    deposit_account: Pubkey,
    vault: Pubkey,
    wrapped_mint: Pubkey,
    nonce: u64,
    cap: u64,
    allowlist: Vec<Pubkey>,
) -> Result<(), String> {
    let (capability, capability_token_account) = capability_for(&principal.pubkey(), nonce);
    let ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::Issue {
            principal: principal.pubkey(),
            principal_deposit_account: deposit_account,
            wrapped_mint,
            vault,
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
            allowlist,
        }
        .data(),
    };
    send(svm, &s.payer, &[principal], &[ix])
}

/// `attenuate` naming an arbitrary wrapped mint — the account the whole file is about.
#[allow(clippy::too_many_arguments)]
fn attenuate_with_mint(
    svm: &mut LiteSVM,
    s: &Setup,
    owner: &Keypair,
    parent_capability: Pubkey,
    child_owner: &Keypair,
    wrapped_mint: Pubkey,
    nonce: u64,
    child_cap: u64,
    child_allowlist: Vec<Pubkey>,
) -> Result<(), String> {
    svm.expire_blockhash();
    svm.airdrop(&child_owner.pubkey(), 1_000_000_000).unwrap();
    let (child_capability, child_token_account) = capability_for(&child_owner.pubkey(), nonce);
    let ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::Attenuate {
            owner: owner.pubkey(),
            parent_capability,
            child_owner: child_owner.pubkey(),
            wrapped_mint,
            program_authority: s.program_authority,
            child_token_account,
            child_capability,
            token_2022_program: spl_token_2022::id(),
            system_program: anchor_lang::solana_program::system_program::ID,
        }
        .to_account_metas(None),
        data: leash_program::instruction::Attenuate {
            nonce,
            child_cap,
            child_expiry: FAR_FUTURE,
            child_allowlist,
        }
        .data(),
    };
    send(svm, &s.payer, &[owner], &[ix])
}

/// Token-2022 `Mint.supply` sits at bytes 36..44 (after `mint_authority: COption<Pubkey>`,
/// which is 4 + 32). Read directly rather than via a helper so the offset is visible.
fn mint_supply(svm: &LiteSVM, mint: &Pubkey) -> u64 {
    let data = svm.get_account(mint).unwrap().data;
    u64::from_le_bytes(data[36..44].try_into().unwrap())
}

/// THE HOLE. A capability issued against a worthless deployment attenuates a child
/// against the *real* wrapped mint, conjuring fully-backed-looking units out of nothing.
#[test]
fn attenuate_rejects_a_foreign_wrapped_mint() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);

    // A legitimate principal deposits 1_000 real USDC. This is the money at risk.
    let principal = Keypair::new();
    expect_ok(issue(&mut svm, &s, &principal, 0, 1_000, FAR_FUTURE, vec![]));
    assert_eq!(token_amount(&svm, &s.vault), 1_000, "real vault funded");
    let real_supply_before = mint_supply(&svm, &s.wrapped_mint);

    // --- The attacker stands up their own, entirely legitimate, worthless deployment. ---
    let attacker = Keypair::new();
    svm.airdrop(&attacker.pubkey(), 10_000_000_000).unwrap();

    let junk_asset = worthless_asset(&mut svm, &s, &attacker);
    let fake_wrapped = mint_with_leash_authority(&mut svm, &s);
    let fake_vault = init_vault(&mut svm, &s, &fake_wrapped, &junk_asset);

    // Fund the attacker with 1_000 units of their own worthless asset.
    let attacker_junk = ata(&attacker.pubkey(), &junk_asset, &spl_token::id());
    send(
        &mut svm,
        &s.payer,
        &[],
        &[create_ata_ix(
            &s.payer.pubkey(),
            &attacker.pubkey(),
            &junk_asset,
            &spl_token::id(),
        )],
    )
    .unwrap();
    send(
        &mut svm,
        &s.payer,
        &[&attacker],
        &[spl_token::instruction::mint_to(
            &spl_token::id(),
            &junk_asset,
            &attacker_junk,
            &attacker.pubkey(),
            &[],
            1_000,
        )
        .unwrap()],
    )
    .unwrap();

    // Where the stolen units are headed. On the attacker's own allowlist, so the hook
    // will wave the spend through.
    let attacker_real_ata = ata(&attacker.pubkey(), &s.wrapped_mint, &spl_token_2022::id());

    // A perfectly valid root capability — backed by 1_000 units of garbage.
    expect_ok(issue_against(
        &mut svm,
        &s,
        &attacker,
        attacker_junk,
        fake_vault,
        fake_wrapped,
        1,
        1_000,
        vec![attacker_real_ata],
    ));
    assert_eq!(
        token_amount(&svm, &fake_vault),
        1_000,
        "the attacker's own vault holds their own garbage — this part is legitimate"
    );

    // --- THE ONE ILLEGITIMATE STEP: attenuate naming the REAL mint. ---
    let (attacker_capability, _) = capability_for(&attacker.pubkey(), 1);
    let agent = Keypair::new();
    let res = attenuate_with_mint(
        &mut svm,
        &s,
        &attacker,
        attacker_capability,
        &agent,
        s.wrapped_mint, // the REAL wrapped mint, not the one this capability was issued against
        2,
        1_000,
        vec![attacker_real_ata],
    );
    expect_err_code(
        res,
        "attenuating against a mint the parent capability was not issued against",
        E_LEASH_WRONG_MINT,
    );

    // The assertions that say the money is still there. An error code alone would not:
    // the whole failure mode is units existing that nothing backs.
    assert_eq!(
        mint_supply(&svm, &s.wrapped_mint),
        real_supply_before,
        "no units of the real mint may be created by a capability issued against another"
    );
    assert_eq!(
        token_amount(&svm, &s.vault),
        1_000,
        "the real vault must be untouched"
    );
}

/// The honest path is unaffected: attenuating against the mint you were actually issued
/// against still works, and still enforces the cap.
#[test]
fn attenuate_still_works_against_the_capabilitys_own_mint() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);

    let principal = Keypair::new();
    let agent = Keypair::new();
    expect_ok(issue(&mut svm, &s, &principal, 0, 1_000, FAR_FUTURE, vec![]));
    let (parent, _) = capability_for(&principal.pubkey(), 0);

    expect_ok(attenuate_with_mint(
        &mut svm,
        &s,
        &principal,
        parent,
        &agent,
        s.wrapped_mint,
        1,
        400,
        vec![],
    ));

    let (_, child_token_account) = capability_for(&agent.pubkey(), 1);
    assert_eq!(token_amount(&svm, &child_token_account), 400);
    assert_eq!(
        capability_state(&svm, &parent).committed_to_children,
        400,
        "the parent's reservation still moves"
    );
}
