//! Shared test infrastructure for leash-program's integration tests. Not a test binary
//! itself — Cargo recognizes `tests/common/mod.rs` as a shared module, not a separate
//! `tests/*.rs` target (unlike a hypothetical `tests/common.rs`, which would be treated
//! as its own test file).
//!
//! `dead_code` is allowed at module level: each test binary only uses a subset of these
//! helpers (e.g. week2 never calls `attenuate`/`spend`), and that's expected for a
//! shared-utilities module, not something to silence by deleting functions other tests
//! rely on.
#![allow(dead_code)]

use anchor_lang::{InstructionData, ToAccountMetas};
use anchor_lang::solana_program::program_pack::Pack;
use anchor_spl::token::spl_token;
use anchor_spl::token_2022::spl_token_2022;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

use anchor_lang::prelude::Pubkey;

pub const PROGRAM_ID: Pubkey = leash_program::ID;
pub const HOOK_ID: Pubkey = leash_hook::ID;

pub fn send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    signers: &[&Keypair],
    ixs: &[anchor_lang::solana_program::instruction::Instruction],
) -> Result<(), String> {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let mut all_signers = vec![payer];
    all_signers.extend_from_slice(signers);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &all_signers).unwrap();
    match svm.send_transaction(tx) {
        Ok(meta) => {
            for line in &meta.logs {
                eprintln!("  log: {}", line);
            }
            Ok(())
        }
        Err(e) => {
            for line in &e.meta.logs {
                eprintln!("  log: {}", line);
            }
            Err(format!("{:?}", e.err))
        }
    }
}

pub fn expect_ok(res: Result<(), String>) {
    assert!(res.is_ok(), "expected success, got error: {:?}", res.err());
}

pub fn expect_err(res: Result<(), String>, label: &str) {
    assert!(res.is_err(), "expected {} to fail, but it succeeded", label);
    eprintln!("  (expected failure for {}: {})", label, res.err().unwrap());
}

/// Anchor's `#[error_code]` numbers variants from 6000, so a `LeashError` /
/// `LeashHookError` variant at index N surfaces on-chain as `Custom(6000 + N)`.
pub const E_LEASH_CAP_EXCEEDED: u32 = 6000; // LeashError::CapExceeded (index 0)
pub const E_LEASH_DEPTH_EXCEEDED: u32 = 6004; // LeashError::DepthExceeded (index 4)
pub const E_LEASH_UNAUTHORIZED: u32 = 6006; // LeashError::Unauthorized (index 6)
pub const E_LEASH_DELEGATED_CANNOT_REDEEM: u32 = 6009;
pub const E_LEASH_NOT_AN_ANCESTOR: u32 = 6010; // LeashError::NotAnAncestor (index 10)
pub const E_LEASH_NOT_A_CHILD: u32 = 6011;
pub const E_LEASH_CHILD_STILL_LIVE: u32 = 6012; // last variant in error.rs
pub const E_HOOK_REVOKED: u32 = 6000; // LeashHookError::Revoked (index 0)
pub const E_HOOK_PARENT_REVOKED: u32 = 6001;
pub const E_HOOK_EXPIRED: u32 = 6002;
pub const E_HOOK_NOT_ALLOWLISTED: u32 = 6003;
pub const E_HOOK_CAP_EXCEEDED: u32 = 6004;

/// `TokenError::InsufficientFunds` — Token-2022's *own* balance check, which fires before
/// leash-hook is ever consulted. Distinct from `E_HOOK_CAP_EXCEEDED`: asserting this one
/// says "the token program stopped it", asserting that one says "leash-hook stopped it".
/// Telling them apart is the whole of docs/ROADMAP.md 0.5.
pub const E_TOKEN_INSUFFICIENT_FUNDS: u32 = 1;

/// Asserts a call failed *for a specific reason*, not merely that it failed.
///
/// `expect_err` above cannot distinguish "leash-hook rejected this" from "Token-2022
/// said insufficient funds" from "the transaction was a byte-identical duplicate" — all
/// three are just `is_err()`. That gap is real: it is how the Week 3 `AlreadyProcessed`
/// false-positives got in, and it is why the cap-vs-balance ambiguity is invisible to
/// the suite (docs/ROADMAP.md 0.5). Prefer this wherever the *cause* of the rejection is
/// the thing under test.
pub fn expect_err_code(res: Result<(), String>, label: &str, code: u32) {
    let err = match res {
        Ok(()) => panic!(
            "expected {} to fail with Custom({}), but it succeeded",
            label, code
        ),
        Err(e) => e,
    };
    let needle = format!("Custom({})", code);
    assert!(
        err.contains(&needle),
        "expected {} to fail with {}, but got: {}",
        label,
        needle,
        err
    );
    eprintln!("  (expected {} for {}: {})", needle, label, err);
}

pub fn token_amount(svm: &LiteSVM, pubkey: &Pubkey) -> u64 {
    let data = svm.get_account(pubkey).unwrap().data;
    u64::from_le_bytes(data[64..72].try_into().unwrap())
}

pub fn capability_state(svm: &LiteSVM, pubkey: &Pubkey) -> leash_program::Capability {
    let data = svm.get_account(pubkey).unwrap().data;
    anchor_lang::AccountDeserialize::try_deserialize(&mut data.as_slice()).unwrap()
}

pub struct Setup {
    pub payer: Keypair,
    pub program_authority: Pubkey,
    pub usdc_mint: Pubkey,
    pub usdc_mint_authority: Keypair,
    pub wrapped_mint: Pubkey,
    pub vault: Pubkey,
}

pub fn setup(svm: &mut LiteSVM) -> Setup {
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 20_000_000_000).unwrap();

    svm.add_program(PROGRAM_ID, include_bytes!("../../../../target/deploy/leash_program.so"))
        .unwrap();
    svm.add_program(HOOK_ID, include_bytes!("../../../../target/deploy/leash_hook.so"))
        .unwrap();

    let (program_authority, _) = Pubkey::find_program_address(&[b"authority"], &PROGRAM_ID);

    // "Real USDC" (legacy SPL Token) mint.
    let usdc_mint = Keypair::new();
    let usdc_mint_authority = Keypair::new();
    let usdc_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
    send(
        svm,
        &payer,
        &[&usdc_mint],
        &[
            anchor_lang::solana_program::system_instruction::create_account(
                &payer.pubkey(),
                &usdc_mint.pubkey(),
                usdc_rent,
                spl_token::state::Mint::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_mint2(
                &spl_token::id(),
                &usdc_mint.pubkey(),
                &usdc_mint_authority.pubkey(),
                None,
                6,
            )
            .unwrap(),
        ],
    )
    .unwrap();

    // Wrapped mint (Token-2022, TransferHook -> leash-hook).
    let wrapped_mint = Keypair::new();
    let extensions = [spl_token_2022::extension::ExtensionType::TransferHook];
    let wrapped_space = spl_token_2022::extension::ExtensionType::try_calculate_account_len::<
        spl_token_2022::state::Mint,
    >(&extensions)
    .unwrap();
    let wrapped_rent = svm.minimum_balance_for_rent_exemption(wrapped_space);
    send(
        svm,
        &payer,
        &[&wrapped_mint],
        &[
            anchor_lang::solana_program::system_instruction::create_account(
                &payer.pubkey(),
                &wrapped_mint.pubkey(),
                wrapped_rent,
                wrapped_space as u64,
                &spl_token_2022::id(),
            ),
            spl_token_2022::extension::transfer_hook::instruction::initialize(
                &spl_token_2022::id(),
                &wrapped_mint.pubkey(),
                Some(program_authority),
                Some(HOOK_ID),
            )
            .unwrap(),
            spl_token_2022::instruction::initialize_mint2(
                &spl_token_2022::id(),
                &wrapped_mint.pubkey(),
                &program_authority,
                None,
                6,
            )
            .unwrap(),
        ],
    )
    .unwrap();

    // Vault: a PDA of the wrapped mint (docs/ROADMAP.md 0.11), created by the program
    // rather than client-side. The derivation is the whole point — it makes this the only
    // account `issue`/`redeem` will accept as the deployment's vault. The previous
    // client-generated keypair could simply be swapped for another one at call time, which
    // is what `deployment_binding.rs` exploits against the old scheme.
    let (vault, _) =
        Pubkey::find_program_address(&[b"vault", wrapped_mint.pubkey().as_ref()], &PROGRAM_ID);
    let init_vault_ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::InitializeVault {
            payer: payer.pubkey(),
            wrapped_mint: wrapped_mint.pubkey(),
            deposit_mint: usdc_mint.pubkey(),
            program_authority,
            vault,
            token_program: spl_token::id(),
            system_program: anchor_lang::solana_program::system_program::ID,
        }
        .to_account_metas(None),
        data: leash_program::instruction::InitializeVault {}.data(),
    };
    send(svm, &payer, &[], &[init_vault_ix]).unwrap();

    // Register the real extra-account-meta list on leash-hook.
    let (extra_account_meta_list, _) =
        Pubkey::find_program_address(&[b"extra-account-metas", wrapped_mint.pubkey().as_ref()], &HOOK_ID);
    let init_extra_metas_ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: HOOK_ID,
        accounts: leash_hook::accounts::InitializeExtraAccountMetaList {
            payer: payer.pubkey(),
            extra_account_meta_list,
            mint: wrapped_mint.pubkey(),
            system_program: anchor_lang::solana_program::system_program::ID,
        }
        .to_account_metas(None),
        data: leash_hook::instruction::InitializeExtraAccountMetaList {}.data(),
    };
    send(svm, &payer, &[], &[init_extra_metas_ix]).unwrap();

    Setup {
        payer,
        program_authority,
        usdc_mint: usdc_mint.pubkey(),
        usdc_mint_authority,
        wrapped_mint: wrapped_mint.pubkey(),
        vault,
    }
}

pub fn ata(owner: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    anchor_spl::associated_token::spl_associated_token_account::address::get_associated_token_address_with_program_id(
        owner, mint, token_program,
    )
}

pub fn create_ata_ix(
    payer: &Pubkey,
    owner: &Pubkey,
    mint: &Pubkey,
    token_program: &Pubkey,
) -> anchor_lang::solana_program::instruction::Instruction {
    anchor_spl::associated_token::spl_associated_token_account::instruction::create_associated_token_account(
        payer, owner, mint, token_program,
    )
}

/// A capability's own wrapped-token account: `[TOKEN_ACCOUNT_SEED, owner, nonce]`. The
/// nonce is what lets one owner hold several capabilities (docs/ROADMAP.md 0.3).
pub fn token_account_pda(owner: &Pubkey, nonce: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[b"capability-token", owner.as_ref(), &nonce.to_le_bytes()],
        &PROGRAM_ID,
    )
    .0
}

/// Keyed on the capability's *token account*, not its owner — the same one fixed formula
/// leash-hook re-derives at transfer time from base account 0.
pub fn capability_pda(token_account: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"capability", token_account.as_ref()], &PROGRAM_ID).0
}

/// Convenience for the common "give me both PDAs for (owner, nonce)" case.
pub fn capability_for(owner: &Pubkey, nonce: u64) -> (Pubkey, Pubkey) {
    let token_account = token_account_pda(owner, nonce);
    (capability_pda(&token_account), token_account)
}

pub fn issue(
    svm: &mut LiteSVM,
    s: &Setup,
    principal: &Keypair,
    nonce: u64,
    cap: u64,
    expiry: i64,
    allowlist: Vec<Pubkey>,
) -> Result<(), String> {
    // This helper predates ROADMAP 0.3 and assumed one issue per principal. Issuing twice
    // to the same principal is now the point, so the funding steps have to tolerate a
    // repeat: the USDC ATA already exists the second time, and the airdrop/mint would
    // otherwise be byte-identical transactions that get rejected as duplicates rather
    // than run. Expiring the blockhash keeps each funding tx distinct; the ATA creation
    // is skipped outright when it is already there.
    svm.expire_blockhash();
    svm.airdrop(&principal.pubkey(), 5_000_000_000).unwrap();
    let principal_usdc = ata(&principal.pubkey(), &s.usdc_mint, &spl_token::id());
    if svm.get_account(&principal_usdc).map_or(true, |a| a.data.is_empty()) {
        send(svm, &s.payer, &[], &[create_ata_ix(&s.payer.pubkey(), &principal.pubkey(), &s.usdc_mint, &spl_token::id())]).unwrap();
    }

    // Fund the principal with at least `cap` USDC to deposit.
    let mint_usdc_ix = spl_token::instruction::mint_to(
        &spl_token::id(),
        &s.usdc_mint,
        &principal_usdc,
        &s.usdc_mint_authority.pubkey(),
        &[],
        cap,
    )
    .unwrap();
    send(svm, &s.payer, &[&s.usdc_mint_authority], &[mint_usdc_ix]).unwrap();

    let (capability, capability_token_account) = capability_for(&principal.pubkey(), nonce);

    let ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::Issue {
            principal: principal.pubkey(),
            principal_deposit_account: principal_usdc,
            vault: s.vault,
            wrapped_mint: s.wrapped_mint,
            program_authority: s.program_authority,
            capability_token_account,
            capability,
            token_program: spl_token::id(),
            token_2022_program: spl_token_2022::id(),
            system_program: anchor_lang::solana_program::system_program::ID,
        }
        .to_account_metas(None),
        data: leash_program::instruction::Issue { nonce, cap, expiry, allowlist }.data(),
    };
    send(svm, &s.payer, &[principal], &[ix])
}

/// `owner` attenuates `parent_capability` (owned by `owner`) into a new child capability
/// held by `child_owner`. Creates the child's ATA and Capability PDA as part of the call.
#[allow(clippy::too_many_arguments)]
pub fn attenuate(
    svm: &mut LiteSVM,
    s: &Setup,
    owner: &Keypair,
    parent_capability: Pubkey,
    child_owner: &Keypair,
    nonce: u64,
    child_cap: u64,
    child_expiry: i64,
    child_allowlist: Vec<Pubkey>,
) -> Result<(), String> {
    // As in `issue`: delegating twice to the same child_owner is exactly what ROADMAP 0.3
    // enables, so a repeated identical airdrop must not collide with the first.
    svm.expire_blockhash();
    svm.airdrop(&child_owner.pubkey(), 1_000_000_000).unwrap();
    let (child_capability, child_token_account) = capability_for(&child_owner.pubkey(), nonce);

    let ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::Attenuate {
            owner: owner.pubkey(),
            parent_capability,
            child_owner: child_owner.pubkey(),
            wrapped_mint: s.wrapped_mint,
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
            child_expiry,
            child_allowlist,
        }
        .data(),
    };
    send(svm, &s.payer, &[owner], &[ix])
}

pub fn revoke(svm: &mut LiteSVM, s: &Setup, owner: &Keypair, capability: Pubkey) -> Result<(), String> {
    let ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::Revoke {
            owner: owner.pubkey(),
            capability,
        }
        .to_account_metas(None),
        data: leash_program::instruction::Revoke {}.data(),
    };
    send(svm, &s.payer, &[owner], &[ix])
}

/// `owner` (holder of `ancestor_capability`) revokes `descendant_capability` somewhere
/// below it in the tree — docs/ROADMAP.md 0.8.
pub fn revoke_descendant(
    svm: &mut LiteSVM,
    s: &Setup,
    owner: &Keypair,
    ancestor_capability: Pubkey,
    descendant_capability: Pubkey,
) -> Result<(), String> {
    // Both of these instructions take no arguments, so two calls against the same
    // accounts serialize to byte-identical transactions and the second is rejected as
    // `AlreadyProcessed` before the program ever runs. That would masquerade as the
    // program refusing — and testing idempotency requires genuinely distinct
    // transactions, so advance the blockhash rather than perturbing the call.
    svm.expire_blockhash();
    let ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::RevokeDescendant {
            owner: owner.pubkey(),
            ancestor_capability,
            descendant_capability,
        }
        .to_account_metas(None),
        data: leash_program::instruction::RevokeDescendant {}.data(),
    };
    send(svm, &s.payer, &[owner], &[ix])
}

/// `owner` (holder of `parent_capability`) releases the budget reserved for a dead
/// `child_capability` — docs/ROADMAP.md 0.7.
pub fn reclaim(
    svm: &mut LiteSVM,
    s: &Setup,
    owner: &Keypair,
    parent_capability: Pubkey,
    child_capability: Pubkey,
) -> Result<(), String> {
    // See the note in `revoke_descendant` — same argument-less duplicate-transaction trap.
    svm.expire_blockhash();
    let ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::Reclaim {
            owner: owner.pubkey(),
            parent_capability,
            child_capability,
        }
        .to_account_metas(None),
        data: leash_program::instruction::Reclaim {}.data(),
    };
    send(svm, &s.payer, &[owner], &[ix])
}

pub fn redeem(
    svm: &mut LiteSVM,
    s: &Setup,
    holder: &Keypair,
    holder_wrapped_account: Pubkey,
    holder_deposit_account: Pubkey,
    amount: u64,
) -> Result<(), String> {
    let ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::Redeem {
            holder: holder.pubkey(),
            holder_wrapped_account,
            // Always passed, existent or not — see redeem.rs's doc comment on why this
            // can't be optional without reopening docs/ROADMAP.md 0.1. Derived from the
            // token account, so a merchant's ATA simply yields an address with nothing
            // at it.
            capability: capability_pda(&holder_wrapped_account),
            wrapped_mint: s.wrapped_mint,
            vault: s.vault,
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

/// `source` is the capability's own token account — an owner may hold several now
/// (docs/ROADMAP.md 0.3), so it can no longer be derived from the owner alone. Use
/// `token_account_pda(owner, nonce)` or the second element of `capability_for`.
pub fn spend(
    svm: &mut LiteSVM,
    s: &Setup,
    source_owner: &Keypair,
    source: &Pubkey,
    destination: &Pubkey,
    amount: u64,
) -> Result<(), String> {
    let source_ata = *source;
    let mut transfer_ix = spl_token_2022::instruction::transfer_checked(
        &spl_token_2022::id(),
        &source_ata,
        &s.wrapped_mint,
        destination,
        &source_owner.pubkey(),
        &[],
        amount,
        6,
    )
    .unwrap();

    // Resolution failure is a real spend failure, not a test-harness bug: if the source
    // has no Capability keyed to it, the hook's seed formula resolves to an account that
    // does not exist and the transfer can never be built. Surface that as an `Err` like
    // any other rejection instead of panicking, so callers can assert on it.
    futures::executor::block_on(
        spl_transfer_hook_interface::offchain::add_extra_account_metas_for_execute(
            &mut transfer_ix,
            &HOOK_ID,
            &source_ata,
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

    send(svm, &s.payer, &[source_owner], &[transfer_ix])
}
