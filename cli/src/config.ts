import { AnchorProvider, Wallet } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import type { Deployment } from "@leash/sdk";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

export function loadKeypair(keypairPath?: string): Keypair {
  const resolved = keypairPath
    ? keypairPath.replace(/^~/, os.homedir())
    : path.join(os.homedir(), ".config", "solana", "id.json");
  const secret = JSON.parse(fs.readFileSync(resolved, "utf-8"));
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

export function loadConnection(url?: string): Connection {
  return new Connection(url || process.env.LEASH_RPC_URL || "http://127.0.0.1:8899", "confirmed");
}

export function loadProvider(url?: string, keypairPath?: string): AnchorProvider {
  const connection = loadConnection(url);
  const wallet = new Wallet(loadKeypair(keypairPath));
  return new AnchorProvider(connection, wallet, { commitment: "confirmed" });
}

const DEFAULT_DEPLOYMENT_FILE = "leash-deployment.json";

export function saveDeployment(deployment: Deployment, file = DEFAULT_DEPLOYMENT_FILE): void {
  fs.writeFileSync(
    file,
    JSON.stringify(
      {
        depositAssetMint: deployment.depositAssetMint.toBase58(),
        wrappedMint: deployment.wrappedMint.toBase58(),
        vault: deployment.vault.toBase58(),
        programAuthority: deployment.programAuthority.toBase58(),
      },
      null,
      2,
    ),
  );
}

export function loadDeployment(file = DEFAULT_DEPLOYMENT_FILE): Deployment {
  if (!fs.existsSync(file)) {
    throw new Error(
      `No deployment file at ${file}. Run \`leash init\` first (see README).`,
    );
  }
  const raw = JSON.parse(fs.readFileSync(file, "utf-8"));
  return {
    depositAssetMint: new PublicKey(raw.depositAssetMint),
    wrappedMint: new PublicKey(raw.wrappedMint),
    vault: new PublicKey(raw.vault),
    programAuthority: new PublicKey(raw.programAuthority),
  };
}
