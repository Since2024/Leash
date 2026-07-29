#!/usr/bin/env node
import {
  attenuate,
  capabilityPda,
  createDeployment,
  fetchCapability,
  loadPrograms,
  mint,
  redeem as redeemCapability,
  revoke as revokeCapability,
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
  .action(async (cmdOpts: { budget: string; expires: string; allow: string }) => {
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
    });
    console.log(`Capability issued: ${result.capability.toBase58()}`);
    console.log(`  wrapped token account: ${result.capabilityTokenAccount.toBase58()}`);
    console.log(`  signature: ${result.signature}`);
  });

program
  .command("attenuate")
  .description("Delegate a smaller, narrower, earlier-expiring child capability.")
  .requiredOption("--child-owner <pubkey>", "Who the new capability is delegated to")
  .requiredOption("--budget <amount>", "Child's cap (must fit in the parent's remaining budget)")
  .requiredOption("--expires <unixSeconds>", "Child's expiry (must be <= parent's)")
  .option("--allow <pubkeys>", "Comma-separated allowlist (must be a subset of the parent's)", "")
  .action(async (cmdOpts: { childOwner: string; budget: string; expires: string; allow: string }) => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const owner = loadKeypair(keypair);
    const deployment = loadDeployment();

    const parentCapability = capabilityPda(owner.publicKey, programs.leashProgramId);
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
    });
    console.log(`Child capability issued: ${result.childCapability.toBase58()}`);
    console.log(`  wrapped token account: ${result.childTokenAccount.toBase58()}`);
    console.log(`  signature: ${result.signature}`);
  });

program
  .command("spend")
  .description("Spend against your own capability (simulates what an agent would do). Enforcement happens on-chain, not here.")
  .requiredOption("--to <tokenAccount>", "Destination wrapped-asset token account")
  .requiredOption("--amount <amount>", "Amount to spend")
  .action(async (cmdOpts: { to: string; amount: string }) => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const owner = loadKeypair(keypair);
    const deployment = loadDeployment();

    const signature = await spendFromCapability(
      programs,
      deployment,
      owner,
      new PublicKey(cmdOpts.to),
      BigInt(cmdOpts.amount),
    );
    console.log(`Spend submitted: ${signature}`);
  });

program
  .command("revoke")
  .description("Revoke your own capability. Takes effect on the very next spend attempt against it or its descendants.")
  .action(async () => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const owner = loadKeypair(keypair);

    const capability = capabilityPda(owner.publicKey, programs.leashProgramId);
    const signature = await revokeCapability(programs, owner, capability);
    console.log(`Revoked ${capability.toBase58()}: ${signature}`);
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
  .command("watch")
  .description("Watch your own capability's live state (spent, revoked, etc.). Ctrl-C to stop.")
  .action(async () => {
    const { url, keypair } = opts();
    const provider = loadProvider(url, keypair);
    const programs = loadPrograms(provider);
    const owner = loadKeypair(keypair);
    const capability = capabilityPda(owner.publicKey, programs.leashProgramId);

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
  revoked: boolean;
  expiry: number;
  depth: number;
}): string {
  const remaining = s.cap - s.spent;
  return `  spent=${s.spent} / cap=${s.cap} (remaining=${remaining})  revoked=${s.revoked}  expiry=${s.expiry}  depth=${s.depth}`;
}

program.parseAsync(process.argv).catch((err) => {
  console.error(err instanceof Error ? err.message : err);
  process.exit(1);
});
