//! Week 2 (docs/BUILD_PLAN.md §7): a full deposit -> issue -> redeem round trip against
//! real Anchor instructions (not stubs), using LiteSVM.
//!
//! Note: `issue`/`redeem` only ever mint_to/transfer/burn — neither touches a transfer,
//! so leash-hook's TransferHook is never invoked here and doesn't need to be loaded in
//! this SVM. It only fires on an actual `transfer`/`transfer_checked` of the wrapped
//! mint, which is Week 3's `spend` path, not this one.

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

const PROGRAM_ID: Pubkey = leash_program::ID;

fn send(svm: &mut LiteSVM, payer: &Keypair, signers: &[&Keypair], ixs: &[anchor_lang::solana_program::instruction::Instruction]) {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let mut all_signers = vec![payer];
    all_signers.extend_from_slice(signers);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &all_signers).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "transaction failed: {:?}", res.err());
    for line in &res.unwrap().logs {
        eprintln!("  log: {}", line);
    }
}

fn token_account_amount(svm: &LiteSVM, pubkey: &Pubkey) -> u64 {
    let data = svm.get_account(pubkey).unwrap().data;
    // SPL Token / Token-2022 base account layout: amount is a u64 at byte offset 64.
    u64::from_le_bytes(data[64..72].try_into().unwrap())
}

#[test]
fn deposit_issue_redeem_round_trip() {
    let mut svm = LiteSVM::new();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    let program_bytes = include_bytes!("../../../target/deploy/leash_program.so");
    svm.add_program(PROGRAM_ID, program_bytes).unwrap();

    let (program_authority, _) =
        Pubkey::find_program_address(&[b"authority"], &PROGRAM_ID);

    // --- Set up the "real USDC" mint (legacy SPL Token) and a principal holding some. ---
    let usdc_mint = Keypair::new();
    let usdc_mint_authority = Keypair::new();
    let usdc_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
    let create_usdc_mint_ix = anchor_lang::solana_program::system_instruction::create_account(
        &payer.pubkey(),
        &usdc_mint.pubkey(),
        usdc_rent,
        spl_token::state::Mint::LEN as u64,
        &spl_token::id(),
    );
    let init_usdc_mint_ix = spl_token::instruction::initialize_mint2(
        &spl_token::id(),
        &usdc_mint.pubkey(),
        &usdc_mint_authority.pubkey(),
        None,
        6,
    )
    .unwrap();
    send(&mut svm, &payer, &[&usdc_mint], &[create_usdc_mint_ix, init_usdc_mint_ix]);

    let principal = Keypair::new();
    svm.airdrop(&principal.pubkey(), 10_000_000_000).unwrap();
    let principal_usdc_account =
        anchor_spl::associated_token::spl_associated_token_account::address::get_associated_token_address_with_program_id(
            &principal.pubkey(),
            &usdc_mint.pubkey(),
            &spl_token::id(),
        );
    let create_principal_usdc_ata_ix =
        anchor_spl::associated_token::spl_associated_token_account::instruction::create_associated_token_account(
            &payer.pubkey(),
            &principal.pubkey(),
            &usdc_mint.pubkey(),
            &spl_token::id(),
        );
    send(&mut svm, &payer, &[], &[create_principal_usdc_ata_ix]);

    let mint_usdc_to_principal_ix = spl_token::instruction::mint_to(
        &spl_token::id(),
        &usdc_mint.pubkey(),
        &principal_usdc_account,
        &usdc_mint_authority.pubkey(),
        &[],
        1_000,
    )
    .unwrap();
    send(&mut svm, &payer, &[&usdc_mint_authority], &[mint_usdc_to_principal_ix]);

    // --- Set up the vault (legacy SPL Token account, owned by program_authority). ---
    let vault = Keypair::new();
    let vault_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Account::LEN);
    let create_vault_ix = anchor_lang::solana_program::system_instruction::create_account(
        &payer.pubkey(),
        &vault.pubkey(),
        vault_rent,
        spl_token::state::Account::LEN as u64,
        &spl_token::id(),
    );
    let init_vault_ix = spl_token::instruction::initialize_account3(
        &spl_token::id(),
        &vault.pubkey(),
        &usdc_mint.pubkey(),
        &program_authority,
    )
    .unwrap();
    send(&mut svm, &payer, &[&vault], &[create_vault_ix, init_vault_ix]);

    // --- Set up the wrapped mint (Token-2022, TransferHook extension). ---
    // Hardcoded rather than depending on the leash-hook crate: this test never triggers
    // a transfer, so the hook is never invoked — only its program ID needs to be
    // recorded on the mint's extension config. Must match leash-hook's declare_id!.
    let wrapped_mint = Keypair::new();
    let hook_id: Pubkey = "9WPQUY6zVRwVZ3eUsDF1aNESWAyZwL8GwKpzd2C66xtS"
        .parse()
        .unwrap();
    let extensions = [spl_token_2022::extension::ExtensionType::TransferHook];
    let wrapped_space = spl_token_2022::extension::ExtensionType::try_calculate_account_len::<
        spl_token_2022::state::Mint,
    >(&extensions)
    .unwrap();
    let wrapped_rent = svm.minimum_balance_for_rent_exemption(wrapped_space);
    let create_wrapped_mint_ix = anchor_lang::solana_program::system_instruction::create_account(
        &payer.pubkey(),
        &wrapped_mint.pubkey(),
        wrapped_rent,
        wrapped_space as u64,
        &spl_token_2022::id(),
    );
    let init_transfer_hook_ix = spl_token_2022::extension::transfer_hook::instruction::initialize(
        &spl_token_2022::id(),
        &wrapped_mint.pubkey(),
        Some(program_authority),
        Some(hook_id),
    )
    .unwrap();
    let init_wrapped_mint_ix = spl_token_2022::instruction::initialize_mint2(
        &spl_token_2022::id(),
        &wrapped_mint.pubkey(),
        &program_authority,
        None,
        6,
    )
    .unwrap();
    send(
        &mut svm,
        &payer,
        &[&wrapped_mint],
        &[create_wrapped_mint_ix, init_transfer_hook_ix, init_wrapped_mint_ix],
    );

    let capability_token_account =
        anchor_spl::associated_token::spl_associated_token_account::address::get_associated_token_address_with_program_id(
            &principal.pubkey(),
            &wrapped_mint.pubkey(),
            &spl_token_2022::id(),
        );
    let (capability, _) = Pubkey::find_program_address(
        &[b"capability", principal.pubkey().as_ref()],
        &PROGRAM_ID,
    );

    // --- issue: deposit 400 USDC, get a $400 capability. ---
    let issue_ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::Issue {
            principal: principal.pubkey(),
            principal_deposit_account: principal_usdc_account,
            vault: vault.pubkey(),
            wrapped_mint: wrapped_mint.pubkey(),
            program_authority,
            capability,
            capability_token_account,
            token_program: spl_token::id(),
            token_2022_program: spl_token_2022::id(),
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: anchor_lang::solana_program::system_program::ID,
        }
        .to_account_metas(None),
        data: leash_program::instruction::Issue {
            cap: 400,
            expiry: 9_999_999_999,
            allowlist: vec![],
        }
        .data(),
    };
    send(&mut svm, &payer, &[&principal], &[issue_ix]);

    assert_eq!(token_account_amount(&svm, &vault.pubkey()), 400);
    assert_eq!(token_account_amount(&svm, &capability_token_account), 400);
    assert_eq!(token_account_amount(&svm, &principal_usdc_account), 600);

    let cap_account = svm.get_account(&capability).unwrap();
    let cap_state: leash_program::Capability =
        anchor_lang::AccountDeserialize::try_deserialize(&mut cap_account.data.as_slice())
            .unwrap();
    assert_eq!(cap_state.owner, principal.pubkey());
    assert_eq!(cap_state.parent, None);
    assert_eq!(cap_state.cap, 400);
    assert_eq!(cap_state.spent, 0);
    assert_eq!(cap_state.committed_to_children, 0);
    assert!(!cap_state.revoked);
    assert_eq!(cap_state.depth, 0);

    // --- redeem: cash 150 of the $400 back out. ---
    let redeem_ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: PROGRAM_ID,
        accounts: leash_program::accounts::Redeem {
            holder: principal.pubkey(),
            holder_wrapped_account: capability_token_account,
            wrapped_mint: wrapped_mint.pubkey(),
            vault: vault.pubkey(),
            program_authority,
            holder_deposit_account: principal_usdc_account,
            token_program: spl_token::id(),
            token_2022_program: spl_token_2022::id(),
        }
        .to_account_metas(None),
        data: leash_program::instruction::Redeem { amount: 150 }.data(),
    };
    send(&mut svm, &payer, &[&principal], &[redeem_ix]);

    assert_eq!(token_account_amount(&svm, &vault.pubkey()), 250);
    assert_eq!(token_account_amount(&svm, &capability_token_account), 250);
    assert_eq!(token_account_amount(&svm, &principal_usdc_account), 750);
}
