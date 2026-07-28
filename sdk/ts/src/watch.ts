import { PublicKey } from "@solana/web3.js";
import type { LeashPrograms } from "./programs";

export interface CapabilitySnapshot {
  owner: PublicKey;
  parent: PublicKey;
  ancestors: PublicKey[];
  tokenAccount: PublicKey;
  cap: bigint;
  spent: bigint;
  committedToChildren: bigint;
  expiry: number;
  allowlist: PublicKey[];
  revoked: boolean;
  depth: number;
}

function decode(programs: LeashPrograms, data: Buffer): CapabilitySnapshot {
  // Anchor's `Program` constructor lowercases the first letter of account names for
  // its JS/TS coder ("Capability" in the Rust struct and raw IDL becomes "capability"
  // here) — confirmed by hitting "Account not found: Capability" against a real
  // Program instance, not assumed from convention.
  const decoded = programs.leashProgram.coder.accounts.decode("capability", data);
  return {
    owner: decoded.owner,
    parent: decoded.parent,
    ancestors: decoded.ancestors,
    tokenAccount: decoded.tokenAccount,
    cap: BigInt(decoded.cap.toString()),
    spent: BigInt(decoded.spent.toString()),
    committedToChildren: BigInt(decoded.committedToChildren.toString()),
    expiry: Number(decoded.expiry.toString()),
    allowlist: decoded.allowlist,
    revoked: decoded.revoked,
    depth: decoded.depth,
  };
}

/** `watch()` — subscribes to a Capability account's live changes (e.g. `spent`
 * increasing as an agent spends, or `revoked` flipping), invoking `onChange` with a
 * decoded snapshot every time the account is updated. Returns a subscription id;
 * pass it to `connection.removeAccountChangeListener` to stop watching. */
export function watch(
  programs: LeashPrograms,
  capability: PublicKey,
  onChange: (snapshot: CapabilitySnapshot) => void,
): number {
  return programs.connection.onAccountChange(
    capability,
    (accountInfo) => {
      onChange(decode(programs, accountInfo.data));
    },
    { commitment: "confirmed" },
  );
}

/** One-shot read, for callers that just want the current state without subscribing. */
export async function fetchCapability(
  programs: LeashPrograms,
  capability: PublicKey,
): Promise<CapabilitySnapshot> {
  const accountInfo = await programs.connection.getAccountInfo(capability, "confirmed");
  if (!accountInfo) {
    throw new Error(`Capability account not found: ${capability.toBase58()}`);
  }
  return decode(programs, accountInfo.data);
}
