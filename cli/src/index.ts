#!/usr/bin/env node
import {
  attenuate,
  capabilityFor,
  createDeployment,
  findCapabilitiesByOwner,
  fetchCapability,
  loadPrograms,
  mint,
  redeem as redeemCapability,
  reclaim as reclaimBudget,
  revoke as revokeCapability,
  revokeDescendant as revokeDescendantCapability,
  spend as spendFromCapability,
  watch,
  type CapabilitySnapshot,
} from "@leash/sdk";
import { PublicKey } from "@solana/web3.js";
import { Command } from "commander";
import { loadDeployment, loadKeypair, loadProvider, saveDeployment } from "./config";

const program = new Command();
program
  .name("leash")
  .description("Spending authority for AI agents as a bearer object.")
  .option("-u, --url <url>", "RPC URL (default: $LEASH_RPC_URL or http://127.0.0.1:8899)")
  .option("-k, --keypair <path>", "Payer/default signer keypair (default: ~/.config/solana/id.json)");

function opts() {
  return program.opts<{ url?: string; keypair?: string }>();
}

/** Names one of the signer's capabilities.
 *
 * An owner can hold any number of them (docs/ROADMAP.md 0.3), so the signer's key alone
 * is no longer enough — every command that acts on an existing capability has to be told
 * which one. Either form works: the capability's address directly, or the nonce it was
 * created with, which re-derives the address locally.
 *
 * There is deliberately no default. Guessing (say, nonce 0) would silently act on the
 * wrong capability for anyone holding several, which is exactly the failure this change
 * exists to prevent. Until 0.6 lands there is no way to list them, so keep the nonce
 * printed by `mint`/`attenuate`. */
function resolveCapability(
  cmdOpts: { capability?: string; nonce?: string },
  owner: PublicKey,
  programId: PublicKey,
): { capability: PublicKey; tokenAccount?: PublicKey } {
  if (cmdOpts.capability && cmdOpts.nonce !== undefined) {
    throw new Error("pass either --capability or --nonce, not both");
  }
  if (cmdOpts.nonce !== undefined) {
    return capabilityFor(owner, BigInt(cmdOpts.nonce), programId);
  }
  if (!cmdOpts.capability) {
    throw new Error(
      "which capability? pass --capability <pubkey> or --nonce <n> (an owner may hold several)",
    );
  }
  // Given an address outright, the token account cannot be worked backwards out of it —
  // the derivation only runs the other way. Commands needing both accept --nonce.
  return { capability: new PublicKey(cmdOpts.capability) };
}

program
  .command("init")
  .description("Set up a new deployment: vault + wrapped mint + hook registration, against an existing deposit-asset mint.")
  .requiredOption("--deposit-mint <pubkey>", "The real asset's mint (e.g. USDC's mint address, or a devnet test mint)")
  .action(async (cmdOpts: { depositMint: string }) => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const payer = loadKeypair(keypair);

    const deployment = await createDeployment(programs, payer, new PublicKey(cmdOpts.depositMint));
    saveDeployment(deployment);
    console.log("Deployment created and saved to leash-deployment.json:");
    console.log(`  deposit asset mint : ${deployment.depositAssetMint.toBase58()}`);
    console.log(`  wrapped mint       : ${deployment.wrappedMint.toBase58()}`);
    console.log(`  vault              : ${deployment.vault.toBase58()}`);
    console.log(`  program authority  : ${deployment.programAuthority.toBase58()}`);
  });

program
  .command("mint")
  .description("Issue a capped, expiring, allowlisted budget (the `issue` instruction).")
  .requiredOption("--budget <amount>", "Total the capability may ever spend")
  .requiredOption("--expires <unixSeconds>", "Unix timestamp after which the capability can no longer spend")
  .option("--allow <pubkeys>", "Comma-separated allowlisted destination token accounts", "")
  .option("--nonce <n>", "Distinguishes this capability from others you hold (random if omitted)")
  .action(async (cmdOpts: { budget: string; expires: string; allow: string; nonce?: string }) => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const principal = loadKeypair(keypair);
    const deployment = loadDeployment();

    const allow = cmdOpts.allow
      ? cmdOpts.allow.split(",").filter(Boolean).map((s) => new PublicKey(s.trim()))
      : [];

    const result = await mint(programs, {
      principal,
      deployment,
      budget: BigInt(cmdOpts.budget),
      expiresAt: Number(cmdOpts.expires),
      allow,
      nonce: cmdOpts.nonce !== undefined ? BigInt(cmdOpts.nonce) : undefined,
    });
    console.log(`Capability issued: ${result.capability.toBase58()}`);
    console.log(`  wrapped token account: ${result.capabilityTokenAccount.toBase58()}`);
    console.log(`  nonce: ${result.nonce}   <- keep this; it re-derives the addresses above`);
    console.log(`  signature: ${result.signature}`);
  });

program
  .command("attenuate")
  .description("Delegate a smaller, narrower, earlier-expiring child capability.")
  .requiredOption("--child-owner <pubkey>", "Who the new capability is delegated to")
  .requiredOption("--budget <amount>", "Child's cap (must fit in the parent's remaining budget)")
  .requiredOption("--expires <unixSeconds>", "Child's expiry (must be <= parent's)")
  .option("--allow <pubkeys>", "Comma-separated allowlist (must be a subset of the parent's)", "")
  .option("--capability <pubkey>", "The parent capability to delegate from")
  .option("--nonce <n>", "Nonce of the parent capability, as an alternative to --capability")
  .option("--child-nonce <n>", "Distinguishes this delegation from others the child holds (random if omitted)")
  .action(async (cmdOpts: { childOwner: string; budget: string; expires: string; allow: string; capability?: string; nonce?: string; childNonce?: string }) => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const owner = loadKeypair(keypair);
    const deployment = loadDeployment();

    const { capability: parentCapability } = resolveCapability(
      cmdOpts,
      owner.publicKey,
      programs.leashProgramId,
    );
    const childOwner = new PublicKey(cmdOpts.childOwner);
    const childAllow = cmdOpts.allow
      ? cmdOpts.allow.split(",").filter(Boolean).map((s) => new PublicKey(s.trim()))
      : [];

    const result = await attenuate(programs, {
      owner,
      parentCapability,
      deployment,
      childOwner,
      childBudget: BigInt(cmdOpts.budget),
      childExpiresAt: Number(cmdOpts.expires),
      childAllow,
      nonce: cmdOpts.childNonce !== undefined ? BigInt(cmdOpts.childNonce) : undefined,
    });
    console.log(`Child capability issued: ${result.childCapability.toBase58()}`);
    console.log(`  wrapped token account: ${result.childTokenAccount.toBase58()}`);
    console.log(`  nonce: ${result.nonce}   <- the child needs this to spend`);
    console.log(`  signature: ${result.signature}`);
  });

program
  .command("spend")
  .description("Spend against your own capability (simulates what an agent would do). Enforcement happens on-chain, not here.")
  .requiredOption("--to <tokenAccount>", "Destination wrapped-asset token account")
  .requiredOption("--amount <amount>", "Amount to spend")
  .option("--from <tokenAccount>", "The capability's own wrapped-token account to spend from")
  .option("--nonce <n>", "Nonce of the capability to spend, as an alternative to --from")
  .action(async (cmdOpts: { to: string; amount: string; from?: string; nonce?: string }) => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const owner = loadKeypair(keypair);
    const deployment = loadDeployment();

    // Which capability gets charged follows entirely from the source account — leash-hook
    // re-derives it from exactly this address — so it has to be named, not guessed.
    let source: PublicKey;
    if (cmdOpts.from && cmdOpts.nonce !== undefined) {
      throw new Error("pass either --from or --nonce, not both");
    } else if (cmdOpts.from) {
      source = new PublicKey(cmdOpts.from);
    } else if (cmdOpts.nonce !== undefined) {
      source = capabilityFor(owner.publicKey, BigInt(cmdOpts.nonce), programs.leashProgramId)
        .tokenAccount;
    } else {
      throw new Error(
        "which capability? pass --from <tokenAccount> or --nonce <n> (an owner may hold several)",
      );
    }

    const signature = await spendFromCapability(
      programs,
      deployment,
      owner,
      source,
      new PublicKey(cmdOpts.to),
      BigInt(cmdOpts.amount),
    );
    console.log(`Spend submitted: ${signature}`);
  });

program
  .command("revoke")
  .description("Revoke one of your capabilities. Takes effect on the very next spend attempt against it or its descendants.")
  .option("--capability <pubkey>", "The capability to revoke")
  .option("--nonce <n>", "Nonce of the capability to revoke, as an alternative to --capability")
  .action(async (cmdOpts: { capability?: string; nonce?: string }) => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const owner = loadKeypair(keypair);

    const { capability } = resolveCapability(cmdOpts, owner.publicKey, programs.leashProgramId);
    const signature = await revokeCapability(programs, owner, capability);
    console.log(`Revoked ${capability.toBase58()}: ${signature}`);
  });

program
  .command("revoke-descendant")
  .description(
    "Revoke a capability below you in the tree (e.g. one agent's allowance) without " +
      "revoking your own, which would cut off every descendant.",
  )
  .requiredOption("--descendant <pubkey>", "The capability to revoke")
  .option("--capability <pubkey>", "Your capability, an ancestor of it")
  .option("--nonce <n>", "Nonce of your capability, as an alternative to --capability")
  .action(async (cmdOpts: { descendant: string; capability?: string; nonce?: string }) => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const owner = loadKeypair(keypair);

    const { capability: ancestor } = resolveCapability(cmdOpts, owner.publicKey, programs.leashProgramId);
    const descendant = new PublicKey(cmdOpts.descendant);
    const signature = await revokeDescendantCapability(programs, owner, ancestor, descendant);
    console.log(`Revoked descendant ${descendant.toBase58()}: ${signature}`);
    console.log("  Note: this stops future spending but does not return the reserved");
    console.log("  budget — run `leash reclaim` for that.");
  });

program
  .command("reclaim")
  .description(
    "Release budget reserved for a child that is revoked or expired, so you can spend " +
      "or redeem it again. Only the immediate parent can do this.",
  )
  .requiredOption("--child <pubkey>", "The child capability to reclaim from")
  .option("--capability <pubkey>", "Your capability (the child's immediate parent)")
  .option("--nonce <n>", "Nonce of your capability, as an alternative to --capability")
  .action(async (cmdOpts: { child: string; capability?: string; nonce?: string }) => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const owner = loadKeypair(keypair);

    const { capability: parent } = resolveCapability(cmdOpts, owner.publicKey, programs.leashProgramId);
    const child = new PublicKey(cmdOpts.child);
    const signature = await reclaimBudget(programs, owner, parent, child);
    console.log(`Reclaimed from ${child.toBase58()}: ${signature}`);
  });

program
  .command("redeem")
  .description(
    "Redeem wrapped units 1:1 for the real deposit asset. Merchants redeem what they " +
      "were paid; a root capability's owner may cash out only budget it hasn't spent or " +
      "delegated. A delegated capability cannot redeem — it can only spend.",
  )
  .requiredOption("--from <tokenAccount>", "Your wrapped-asset token account to burn from")
  .requiredOption("--to <tokenAccount>", "Your real-asset token account to receive into")
  .requiredOption("--amount <amount>", "Amount to redeem")
  .action(async (cmdOpts: { from: string; to: string; amount: string }) => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const holder = loadKeypair(keypair);
    const deployment = loadDeployment();

    const signature = await redeemCapability(
      programs,
      holder,
      deployment,
      new PublicKey(cmdOpts.from),
      new PublicKey(cmdOpts.to),
      BigInt(cmdOpts.amount),
    );
    console.log(`Redeemed: ${signature}`);
  });

program
  .command("list")
  .description("List every capability you own, by scanning program accounts.")
  .option(
    "--recover-nonce <limit>",
    "Also try to recover each nonce by testing 0..limit (only finds small sequential nonces)",
  )
  .action(async (cmdOpts: { recoverNonce?: string }) => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const owner = loadKeypair(keypair);

    const found = await findCapabilitiesByOwner(programs, owner.publicKey, {
      recoverNonceLimit:
        cmdOpts.recoverNonce !== undefined ? Number(cmdOpts.recoverNonce) : undefined,
    });
    if (found.length === 0) {
      console.log("No capabilities found for this key.");
      return;
    }
    console.log(`${found.length} capability/capabilities owned by ${owner.publicKey.toBase58()}:`);
    for (const f of found) {
      // The nonce is the one thing a scan cannot reliably recover — it is hashed into an
      // address, not stored — so say so plainly rather than printing a blank column.
      const nonce =
        f.nonce !== undefined
          ? `nonce=${f.nonce}`
          : cmdOpts.recoverNonce !== undefined
            ? "nonce=? (not in scanned range)"
            : "nonce=? (pass --recover-nonce <limit> to try)";
      console.log(`  ${f.address.toBase58()}  ${nonce}`);
      console.log(`    ${formatSnapshot(f.state)}`);
      console.log(`    token account: ${f.state.tokenAccount.toBase58()}`);
    }
  });

program
  .command("watch")
  .description("Watch one of your capabilities' live state (spent, revoked, etc.). Ctrl-C to stop.")
  .option("--capability <pubkey>", "The capability to watch")
  .option("--nonce <n>", "Nonce of the capability to watch, as an alternative to --capability")
  .action(async (cmdOpts: { capability?: string; nonce?: string }) => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const owner = loadKeypair(keypair);
    const { capability } = resolveCapability(cmdOpts, owner.publicKey, programs.leashProgramId);

    const initial = await fetchCapability(programs, capability);
    console.log(`Watching ${capability.toBase58()} (Ctrl-C to stop):`);
    console.log(formatSnapshot(initial));

    watch(programs, capability, (snapshot: CapabilitySnapshot) => {
      console.log(formatSnapshot(snapshot));
    });
  });

function formatSnapshot(s: {
  cap: bigint;
  spent: bigint;
  committedToChildren: bigint;
  revoked: boolean;
  expiry: number;
  depth: number;
}): string {
  // Must subtract `committedToChildren` as well as `spent`. Budget delegated to a child
  // is not spendable by this capability — the hook rejects it with CapExceeded — so
  // reporting `cap - spent` overstates what the holder can actually spend by exactly the
  // amount currently delegated. Caught by watching the CLI print "remaining=1000" for a
  // capability whose next spend of 851 was rejected on-chain.
  const spendable = s.cap - s.spent - s.committedToChildren;
  const delegated =
    s.committedToChildren > 0n ? `  delegated=${s.committedToChildren}` : "";
  return `  spent=${s.spent} / cap=${s.cap} (spendable=${spendable})${delegated}  revoked=${s.revoked}  expiry=${s.expiry}  depth=${s.depth}`;
}

program.parseAsync(process.argv).catch((err) => {
  console.error(err instanceof Error ? err.message : err);
  process.exit(1);
});
