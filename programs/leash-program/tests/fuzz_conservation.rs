//! Randomized property testing over *sequences* of instructions (docs/ROADMAP.md, Phase 1).
//!
//! Everything else in this directory is a hand-built case: a specific story, asserted at
//! specific points. That is what `tests/invariants/README.md` means by "fuzz suite", and
//! it says so honestly. This file is the other thing — it drives random operation
//! sequences against a growing capability tree and checks the invariants after *every*
//! operation, including operations that fail.
//!
//! Two properties are checked, and the second is the one that matters:
//!
//! 1. **Per-node conservation**, the invariant `state.rs` advertises and BUILD_PLAN.md §2
//!    promises: `spent + committed_to_children <= cap`, for every capability, always.
//!    This is what 0.2 violated.
//!
//! 2. **Global solvency**: the sum of every claim that can still legitimately reach the
//!    vault must never exceed the vault's balance. This is strictly stronger, and it is
//!    the property depositors actually care about — per-node conservation could hold at
//!    every node while the tree as a whole still promised more than it could pay. It is
//!    also what 0.1 violated (via redemption) without breaking property 1 anywhere.
//!
//! Failures are reproducible: the RNG is a seeded xorshift written out below rather than
//! a dependency, every assertion message carries the seed and step number, and the same
//! seed replays the same sequence exactly.
//!
//! Operations are *allowed to fail*. A rejected spend, a duplicate transaction, an
//! attenuation past MAX_DEPTH — all expected, none assertion-worthy. The point is that
//! the invariants survive whatever the program does in response, not that any particular
//! call succeeds.

mod common;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::clock::Clock;
use anchor_spl::token::spl_token;
use anchor_spl::token_2022::spl_token_2022;
use common::*;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;

/// xorshift64*, deterministic and dependency-free. Quality is irrelevant here; replayable
/// sequences are not.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Avoid the zero fixed point, which would emit zeros forever.
        Rng(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform-ish in `[0, n)`.
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        if hi <= lo { lo } else { lo + self.below(hi - lo) }
    }
}

struct Node {
    owner: Keypair,
    capability: Pubkey,
    token_account: Pubkey,
    /// Index into `nodes` of the immediate parent; `None` for a root.
    parent: Option<usize>,
}

/// The two invariants, evaluated against live on-chain state.
///
/// `label` carries the seed and step so a failure names the exact sequence to replay.
fn check_invariants(svm: &LiteSVM, s: &Setup, nodes: &[Node], merchant_ata: &Pubkey, label: &str) {
    let vault_balance = token_amount(svm, &s.vault);
    let now = svm.get_sysvar::<Clock>().unix_timestamp;

    // Anything a merchant holds can be redeemed unconditionally (docs/ROADMAP.md 0.1).
    let mut claims: u128 = token_amount(svm, merchant_ata) as u128;

    for (i, n) in nodes.iter().enumerate() {
        let c = capability_state(svm, &n.capability);

        // Property 1, per node.
        assert!(
            c.spent as u128 + c.committed_to_children as u128 <= c.cap as u128,
            "{label}: conservation violated at node {i} ({}): spent={} committed={} cap={}",
            n.capability,
            c.spent,
            c.committed_to_children,
            c.cap,
        );

        let free = (c.cap as u128)
            .saturating_sub(c.spent as u128)
            .saturating_sub(c.committed_to_children as u128);
        let balance = token_amount(svm, &n.token_account) as u128;
        let dead = c.revoked || now > c.expiry;

        // A root can always unwind its free budget: `redeem` gates on `depth == 0` alone
        // and reads neither `revoked` nor `expiry`. A delegated capability that is dead
        // can do nothing at all — it cannot spend (the hook checks both flags) and cannot
        // redeem (`DelegatedCannotRedeem`), so it contributes no claim.
        claims += if c.depth == 0 {
            free
        } else if dead {
            0
        } else {
            free.min(balance)
        };
    }

    // Property 2, globally.
    assert!(
        claims <= vault_balance as u128,
        "{label}: solvency violated: outstanding claims {claims} exceed vault {vault_balance}",
    );
}

/// One randomized run. Returns the number of operations that actually landed, purely so
/// the test can assert the sequence wasn't vacuous.
fn run_sequence(seed: u64, steps: usize) -> usize {
    let mut rng = Rng::new(seed);
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);

    // One merchant, allowlisted everywhere, holding a USDC account so it can redeem.
    let merchant = Keypair::new();
    let merchant_ata = ata(&merchant.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    let merchant_usdc = ata(&merchant.pubkey(), &s.usdc_mint, &spl_token::id());
    send(
        &mut svm,
        &s.payer,
        &[],
        &[
            create_ata_ix(&s.payer.pubkey(), &merchant.pubkey(), &s.wrapped_mint, &spl_token_2022::id()),
            create_ata_ix(&s.payer.pubkey(), &merchant.pubkey(), &s.usdc_mint, &spl_token::id()),
        ],
    )
    .unwrap();
    svm.airdrop(&merchant.pubkey(), 1_000_000_000).unwrap();

    let far_future: i64 = 4_102_444_800;
    let mut nodes: Vec<Node> = Vec::new();
    let mut nonce: u64 = 0;
    let mut landed = 0usize;

    for step in 0..steps {
        let label = format!("seed={seed} step={step}");
        // Weighted by rough interest: spending and delegating dominate, since they are
        // what move the numbers the invariants are about.
        let op = rng.below(100);

        match op {
            // --- issue a new root ---
            0..=9 => {
                let principal = Keypair::new();
                let cap = rng.range(100, 2_000);
                nonce += 1;
                if issue(&mut svm, &s, &principal, nonce, cap, far_future, vec![merchant_ata]).is_ok()
                {
                    let (capability, token_account) = capability_for(&principal.pubkey(), nonce);
                    nodes.push(Node { owner: principal, capability, token_account, parent: None });
                    landed += 1;
                }
            }

            // --- attenuate from an existing node ---
            10..=39 => {
                if nodes.is_empty() {
                    continue;
                }
                let pi = rng.below(nodes.len() as u64) as usize;
                let parent_cap = nodes[pi].capability;
                let parent_state = capability_state(&svm, &parent_cap);
                // Deliberately not clamped to the parent's remaining budget: over-asking
                // must be rejected by the program, and that rejection is part of what is
                // being fuzzed.
                let child_cap = rng.range(1, parent_state.cap.max(2));
                // Sometimes a short expiry, so expiry-driven death gets exercised too.
                let child_expiry = if rng.below(4) == 0 {
                    svm.get_sysvar::<Clock>().unix_timestamp + rng.range(1, 50) as i64
                } else {
                    far_future
                };
                let child_owner = Keypair::new();
                nonce += 1;
                let ok = attenuate(
                    &mut svm,
                    &s,
                    &nodes[pi].owner,
                    parent_cap,
                    &child_owner,
                    nonce,
                    child_cap,
                    child_expiry,
                    vec![merchant_ata],
                )
                .is_ok();
                if ok {
                    let (capability, token_account) = capability_for(&child_owner.pubkey(), nonce);
                    nodes.push(Node {
                        owner: child_owner,
                        capability,
                        token_account,
                        parent: Some(pi),
                    });
                    landed += 1;
                }
            }

            // --- spend ---
            40..=74 => {
                if nodes.is_empty() {
                    continue;
                }
                let i = rng.below(nodes.len() as u64) as usize;
                let c = capability_state(&svm, &nodes[i].capability);
                // Straddle the boundary on purpose: sometimes inside the budget,
                // sometimes past it.
                let free = c.cap.saturating_sub(c.spent).saturating_sub(c.committed_to_children);
                let amount = if rng.below(3) == 0 {
                    free.saturating_add(rng.range(1, 50))
                } else {
                    rng.range(1, free.max(2))
                };
                let (source, dest) = (nodes[i].token_account, merchant_ata);
                if spend(&mut svm, &s, &nodes[i].owner, &source, &dest, amount).is_ok() {
                    landed += 1;
                }
            }

            // --- revoke self ---
            75..=79 => {
                if nodes.is_empty() {
                    continue;
                }
                let i = rng.below(nodes.len() as u64) as usize;
                let cap = nodes[i].capability;
                if revoke(&mut svm, &s, &nodes[i].owner, cap).is_ok() {
                    landed += 1;
                }
            }

            // --- an ancestor revokes a descendant ---
            80..=86 => {
                if nodes.is_empty() {
                    continue;
                }
                let i = rng.below(nodes.len() as u64) as usize;
                if let Some(pi) = nodes[i].parent {
                    let (ancestor, descendant) = (nodes[pi].capability, nodes[i].capability);
                    if revoke_descendant(&mut svm, &s, &nodes[pi].owner, ancestor, descendant).is_ok()
                    {
                        landed += 1;
                    }
                }
            }

            // --- reclaim a child's reservation ---
            87..=93 => {
                if nodes.is_empty() {
                    continue;
                }
                let i = rng.below(nodes.len() as u64) as usize;
                if let Some(pi) = nodes[i].parent {
                    let (parent_cap, child_cap) = (nodes[pi].capability, nodes[i].capability);
                    if reclaim(&mut svm, &s, &nodes[pi].owner, parent_cap, child_cap).is_ok() {
                        landed += 1;
                    }
                }
            }

            // --- redeem (root unwinding, or the merchant cashing out) ---
            94..=97 => {
                if rng.below(2) == 0 {
                    let amount = rng.range(1, 200);
                    if redeem(
                        &mut svm,
                        &s,
                        &merchant,
                        merchant_ata,
                        merchant_usdc,
                        amount,
                    )
                    .is_ok()
                    {
                        landed += 1;
                    }
                } else if !nodes.is_empty() {
                    let i = rng.below(nodes.len() as u64) as usize;
                    let holder_usdc = ata(&nodes[i].owner.pubkey(), &s.usdc_mint, &spl_token::id());
                    let (ta, amount) = (nodes[i].token_account, rng.range(1, 200));
                    if redeem(&mut svm, &s, &nodes[i].owner, ta, holder_usdc, amount).is_ok() {
                        landed += 1;
                    }
                }
            }

            // --- let time pass, so expiry actually bites ---
            _ => {
                let mut clock: Clock = svm.get_sysvar();
                clock.unix_timestamp += rng.range(1, 100) as i64;
                svm.set_sysvar(&clock);
                landed += 1;
            }
        }

        check_invariants(&svm, &s, &nodes, &merchant_ata, &label);
    }

    // A run where nothing ever succeeded would pass both invariants vacuously.
    assert!(
        nodes.len() >= 2,
        "seed={seed}: only {} capabilities were created; the sequence was too degenerate to prove anything",
        nodes.len()
    );
    landed
}

/// Several independent sequences. Seeds are fixed rather than time-derived so that a
/// failure in CI is reproducible from the log alone.
#[test]
fn conservation_and_solvency_survive_random_sequences() {
    let mut total = 0usize;
    for seed in [1u64, 7, 42, 1337, 90210, 2_718_281_828] {
        total += run_sequence(seed, 120);
    }
    assert!(total > 100, "sequences landed only {total} operations in total");
    eprintln!("fuzz: {total} operations landed across 6 seeds");
}

/// A deeper single run, to reach states short sequences rarely build (deep trees, several
/// reclaims against the same parent, expiry interleaved with delegation).
#[test]
fn conservation_survives_a_long_sequence() {
    let landed = run_sequence(0xDEAD_BEEF, 400);
    assert!(landed > 50, "long sequence landed only {landed} operations");
    eprintln!("fuzz: long sequence landed {landed} operations");
}

/// A wide sweep, kept out of the default run because it takes minutes rather than seconds.
///
/// `cargo test -p leash-program --test fuzz_conservation -- --ignored --nocapture`
///
/// The two tests above are the regression gate — they pin seeds already known to explore
/// interesting states, including seed 1, which is what caught `reclaim` releasing budget
/// that was still committed to a grandchild. This one is for *finding* the next such bug:
/// run it after any change to the accounting paths (`attenuate`, `record_spend`, `redeem`,
/// `reclaim`), and if it fails, copy the failing seed into the list above so the case is
/// pinned forever rather than rediscovered by luck.
#[test]
#[ignore = "slow: wide seed sweep, run explicitly with --ignored"]
fn conservation_survives_a_wide_seed_sweep() {
    let mut total = 0usize;
    for seed in 1..=60u64 {
        total += run_sequence(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15), 150);
    }
    eprintln!("fuzz: wide sweep landed {total} operations across 60 seeds");
}
