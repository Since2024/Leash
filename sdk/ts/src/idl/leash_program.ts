/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/leash_program.json`.
 */
export type LeashProgram = {
  "address": "Gbx7nEL2rxWUTj7LnqRQtBDU7yi8oF3miYmjKGncsDXk",
  "metadata": {
    "name": "leashProgram",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Leash: capability accounts, issue/attenuate/revoke/redeem. See docs/BUILD_PLAN.md."
  },
  "docs": [
    "Capability program: issue / attenuate / revoke / redeem, plus `record_spend` — the",
    "CPI entrypoint leash-hook calls to commit a spend it has already validated. The",
    "actual cap/expiry/allowlist/revoked checks happen in leash-hook, inside the",
    "Token-2022 transfer itself (docs/BUILD_PLAN.md §4); this program owns the Capability",
    "accounts and is the only one allowed to write `spent`."
  ],
  "instructions": [
    {
      "name": "attenuate",
      "docs": [
        "`nonce` plays the same role as in `issue`, scoped to `child_owner` — which is what",
        "lets one parent delegate to the same owner more than once."
      ],
      "discriminator": [
        56,
        166,
        252,
        115,
        212,
        96,
        92,
        187
      ],
      "accounts": [
        {
          "name": "owner",
          "writable": true,
          "signer": true,
          "relations": [
            "parentCapability"
          ]
        },
        {
          "name": "parentCapability",
          "writable": true
        },
        {
          "name": "childOwner",
          "docs": [
            "the parent's owner is the one authorizing this attenuation, not the child."
          ]
        },
        {
          "name": "wrappedMint",
          "docs": [
            "leash-wrapped-USD mint. Mutable because minting increases supply. Typed so Anchor",
            "can read its extensions to size the child's token account."
          ],
          "writable": true
        },
        {
          "name": "programAuthority",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "childTokenAccount",
          "docs": [
            "The child's own wrapped-token account, at `[TOKEN_ACCOUNT_SEED, child_owner,",
            "nonce]`, owned by `child_owner` directly (same bearer-object model as `issue`).",
            "Declared before `child_capability`, whose seeds reference it."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  97,
                  112,
                  97,
                  98,
                  105,
                  108,
                  105,
                  116,
                  121,
                  45,
                  116,
                  111,
                  107,
                  101,
                  110
                ]
              },
              {
                "kind": "account",
                "path": "childOwner"
              },
              {
                "kind": "arg",
                "path": "nonce"
              }
            ]
          }
        },
        {
          "name": "childCapability",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  97,
                  112,
                  97,
                  98,
                  105,
                  108,
                  105,
                  116,
                  121
                ]
              },
              {
                "kind": "account",
                "path": "childTokenAccount"
              }
            ]
          }
        },
        {
          "name": "token2022Program"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "nonce",
          "type": "u64"
        },
        {
          "name": "childCap",
          "type": "u64"
        },
        {
          "name": "childExpiry",
          "type": "i64"
        },
        {
          "name": "childAllowlist",
          "type": {
            "vec": "pubkey"
          }
        }
      ]
    },
    {
      "name": "initializeVault",
      "docs": [
        "Creates the single vault backing `wrapped_mint`, at `[VAULT_SEED, wrapped_mint]`.",
        "Must be called once per deployment, in the same transaction that creates the mint —",
        "see initialize_vault.rs. docs/ROADMAP.md 0.11."
      ],
      "discriminator": [
        48,
        191,
        163,
        44,
        71,
        129,
        63,
        164
      ],
      "accounts": [
        {
          "name": "payer",
          "writable": true,
          "signer": true
        },
        {
          "name": "wrappedMint",
          "docs": [
            "The Token-2022 wrapped mint this vault backs. Required to be a mint whose mint",
            "authority is this program's `program_authority` PDA — that is what makes it a real",
            "leash wrapped mint rather than an arbitrary token, and it keeps the set of vaults",
            "that can ever exist tied to actual deployments."
          ]
        },
        {
          "name": "depositMint",
          "docs": [
            "The real deposited asset (legacy SPL Token), e.g. USDC."
          ]
        },
        {
          "name": "programAuthority",
          "docs": [
            "read."
          ],
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "vault",
          "docs": [
            "`init` here is what makes the address canonical: exactly one vault can ever exist",
            "per wrapped mint, and a second call fails on the account already being in use."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "wrappedMint"
              }
            ]
          }
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": []
    },
    {
      "name": "issue",
      "docs": [
        "`nonce` distinguishes this capability's token account from any other the same",
        "principal holds, and so distinguishes the capability itself — see",
        "constants::CAPABILITY_SEED. Callers who don't care may pass any value they",
        "haven't used before; the SDK generates a random `u64`."
      ],
      "discriminator": [
        190,
        1,
        98,
        214,
        81,
        99,
        222,
        247
      ],
      "accounts": [
        {
          "name": "principal",
          "writable": true,
          "signer": true
        },
        {
          "name": "principalDepositAccount",
          "writable": true
        },
        {
          "name": "wrappedMint",
          "docs": [
            "leash-wrapped-USD mint (Token-2022, TransferHook extension already configured at",
            "deployment time). Mutable because minting increases supply. Typed rather than",
            "unchecked so Anchor can read its extensions to size the token account below.",
            "",
            "Declared *before* `vault` because the vault's seeds reference it, and Anchor",
            "resolves fields in declaration order. That ordering is the fix for",
            "docs/ROADMAP.md 0.11 — see the vault below."
          ],
          "writable": true
        },
        {
          "name": "vault",
          "docs": [
            "The program's vault: the legacy SPL Token account holding the real deposited",
            "asset, created once per wrapped mint by `initialize_vault`.",
            "",
            "The `seeds` constraint is load-bearing and was absent (docs/ROADMAP.md 0.11). As a",
            "bare `UncheckedAccount` this took whatever vault the caller named, so a caller",
            "could deposit into an account they owned and still be minted genuine wrapped units",
            "against the real mint — a fully-backed-looking capability with nothing behind it,",
            "redeemable from the real vault by the ordinary path. Deriving the vault from",
            "`wrapped_mint` makes the deposit and the mint provably part of the same",
            "deployment."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "wrappedMint"
              }
            ]
          }
        },
        {
          "name": "programAuthority",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "capabilityTokenAccount",
          "docs": [
            "This capability's own wrapped-token account, at `[TOKEN_ACCOUNT_SEED, principal,",
            "nonce]`. Declared *before* `capability` because the capability's seeds reference",
            "this account's key, and Anchor resolves fields in declaration order.",
            "",
            "It used to be the principal's associated token account, created by hand via a CPI",
            "to the ATA program. An ATA is unique per (owner, mint), so it could only ever",
            "represent one capability — the root of docs/ROADMAP.md 0.3. Anchor's `init` +",
            "`token::*` constraints replace that CPI outright and size the account for the",
            "mint's TransferHook extension automatically.",
            "",
            "`token::authority = principal` keeps the bearer-object model from BUILD_PLAN.md §0",
            "intact: the holder controls the account directly, leash-program does not gate",
            "access to it. Only the *address* is program-derived, not the authority."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  97,
                  112,
                  97,
                  98,
                  105,
                  108,
                  105,
                  116,
                  121,
                  45,
                  116,
                  111,
                  107,
                  101,
                  110
                ]
              },
              {
                "kind": "account",
                "path": "principal"
              },
              {
                "kind": "arg",
                "path": "nonce"
              }
            ]
          }
        },
        {
          "name": "capability",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  97,
                  112,
                  97,
                  98,
                  105,
                  108,
                  105,
                  116,
                  121
                ]
              },
              {
                "kind": "account",
                "path": "capabilityTokenAccount"
              }
            ]
          }
        },
        {
          "name": "tokenProgram",
          "docs": [
            "Typed rather than unchecked: both of these are CPI targets, and an unchecked",
            "account that gets invoked is the same bug class as an unchecked vault. The",
            "instruction builders in `spl-token`/`spl-token-2022` happen to reject a foreign",
            "program id themselves, so this is belt-and-braces — but it makes the requirement",
            "visible at the account list instead of buried in a dependency's internals."
          ],
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        },
        {
          "name": "token2022Program",
          "address": "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "nonce",
          "type": "u64"
        },
        {
          "name": "cap",
          "type": "u64"
        },
        {
          "name": "expiry",
          "type": "i64"
        },
        {
          "name": "allowlist",
          "type": {
            "vec": "pubkey"
          }
        }
      ]
    },
    {
      "name": "reclaim",
      "docs": [
        "Release budget reserved for a child that can no longer spend it, so the parent can",
        "use or redeem it again. Accounting only — see reclaim.rs for why the child's units",
        "cannot be burned. docs/ROADMAP.md 0.7."
      ],
      "discriminator": [
        44,
        177,
        236,
        249,
        145,
        109,
        163,
        186
      ],
      "accounts": [
        {
          "name": "owner",
          "signer": true,
          "relations": [
            "parentCapability"
          ]
        },
        {
          "name": "parentCapability",
          "writable": true
        },
        {
          "name": "childCapability",
          "docs": [
            "Mutated: its `cap` is written down so the same budget cannot be reclaimed twice."
          ],
          "writable": true
        }
      ],
      "args": []
    },
    {
      "name": "recordSpend",
      "discriminator": [
        111,
        102,
        17,
        64,
        245,
        202,
        79,
        55
      ],
      "accounts": [
        {
          "name": "hookAuthority",
          "docs": [
            "constraint can't reference a non-`Pubkey::find_program_address`-under-this-program",
            "scheme directly, so the check is manual in the handler). `signer` is required",
            "here, not just checked manually: without it, Anchor's account-type (a plain",
            "`UncheckedAccount`, not `Signer`) makes the *generated CPI instruction itself*",
            "mark `is_signer: false` in its account metas, so `invoke_signed`'s PDA signature",
            "never gets a chance to matter — confirmed by hitting `is_signer == false` here",
            "with a correctly-matching PDA address, not by guessing."
          ],
          "signer": true
        },
        {
          "name": "capability",
          "writable": true
        }
      ],
      "args": [
        {
          "name": "amount",
          "type": "u64"
        }
      ]
    },
    {
      "name": "redeem",
      "discriminator": [
        184,
        12,
        86,
        149,
        70,
        196,
        97,
        225
      ],
      "accounts": [
        {
          "name": "holder",
          "signer": true
        },
        {
          "name": "holderWrappedAccount",
          "docs": [
            "Declared before `capability`, whose seeds derive from it."
          ],
          "writable": true
        },
        {
          "name": "capability",
          "docs": [
            "the seeds constraint. May legitimately not exist (a merchant holding received",
            "units) — the handler distinguishes the two by inspecting owner/data rather than",
            "trusting the caller, so this cannot be typed as `Account<Capability>` or",
            "`Option<_>`. Mutable because a root's redemption writes `cap` back down.",
            "",
            "Derived from the token account rather than the holder (docs/ROADMAP.md 0.3), which",
            "makes the association exact: if a capability exists at this address then this",
            "account *is* its token account, so the \"is this unspent budget?\" question is",
            "answered by the derivation itself. Under the old owner-keyed scheme it could only",
            "be answered as \"the one capability this holder happens to have,\" which stops being",
            "good enough the moment an owner can hold several."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  97,
                  112,
                  97,
                  98,
                  105,
                  108,
                  105,
                  116,
                  121
                ]
              },
              {
                "kind": "account",
                "path": "holderWrappedAccount"
              }
            ]
          }
        },
        {
          "name": "wrappedMint",
          "docs": [
            "because it needs no field read — what makes it trustworthy is that the vault below",
            "is derived from it, so naming a counterfeit mint here names a vault that is not the",
            "real one."
          ],
          "writable": true
        },
        {
          "name": "vault",
          "docs": [
            "Program vault (legacy SPL Token, real USDC) — source of the withdrawal, and the",
            "account docs/ROADMAP.md 0.11 was about.",
            "",
            "This was a bare `UncheckedAccount`, and the combination with an unchecked",
            "`wrapped_mint` was the worst hole in the program: the burn took whatever mint the",
            "caller named and the payout came from whatever vault the caller named, with",
            "nothing requiring the two to be related. Burn a Token-2022 mint you created",
            "yourself, name the real vault, and the program pays you out of a stranger's",
            "deposit — one instruction, no capability, no prior state. `tests/deployment_binding.rs`",
            "demonstrates it against the real binary.",
            "",
            "`program_authority`'s own `seeds` constraint does not help, and the reason is the",
            "instructive part: it proves the *signer* is canonical, not which account that",
            "signer is being made to pay out of. It is seeded `[AUTHORITY_SEED]` with no mint in",
            "the seeds, so one PDA is the authority for every vault the program will ever have.",
            "",
            "Deriving the vault from `wrapped_mint` is what ties them together: the seeds that",
            "say which mint say which vault."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "wrappedMint"
              }
            ]
          }
        },
        {
          "name": "programAuthority",
          "docs": [
            "one shared authority for the deployment (see constants::AUTHORITY_SEED)."
          ],
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "holderDepositAccount",
          "writable": true
        },
        {
          "name": "tokenProgram",
          "docs": [
            "Typed for the same reason as in `issue`: these are CPI targets, and an unchecked",
            "account that gets invoked is the same bug class as an unchecked vault."
          ],
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        },
        {
          "name": "token2022Program",
          "address": "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        }
      ],
      "args": [
        {
          "name": "amount",
          "type": "u64"
        }
      ]
    },
    {
      "name": "revoke",
      "discriminator": [
        170,
        23,
        31,
        34,
        133,
        173,
        93,
        242
      ],
      "accounts": [
        {
          "name": "owner",
          "signer": true,
          "relations": [
            "capability"
          ]
        },
        {
          "name": "capability",
          "writable": true
        }
      ],
      "args": []
    },
    {
      "name": "revokeDescendant",
      "docs": [
        "Revoke a capability below you in the tree. Authority is proved by the target's own",
        "`ancestors` array — the same one leash-hook already gates spends on. See",
        "docs/ROADMAP.md 0.8."
      ],
      "discriminator": [
        105,
        73,
        97,
        174,
        22,
        162,
        90,
        196
      ],
      "accounts": [
        {
          "name": "owner",
          "signer": true,
          "relations": [
            "ancestorCapability"
          ]
        },
        {
          "name": "ancestorCapability",
          "docs": [
            "The signer's own capability, somewhere above `descendant_capability` in the tree.",
            "Not mutated — this is the proof of authority, not the target."
          ]
        },
        {
          "name": "descendantCapability",
          "writable": true
        }
      ],
      "args": []
    }
  ],
  "accounts": [
    {
      "name": "capability",
      "discriminator": [
        192,
        140,
        41,
        92,
        236,
        64,
        181,
        99
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "capExceeded",
      "msg": "spend would exceed this capability's remaining budget"
    },
    {
      "code": 6001,
      "name": "expired",
      "msg": "capability has expired"
    },
    {
      "code": 6002,
      "name": "notAllowlisted",
      "msg": "destination is not on this capability's allowlist"
    },
    {
      "code": 6003,
      "name": "revoked",
      "msg": "capability, or an ancestor of it, has been revoked"
    },
    {
      "code": 6004,
      "name": "depthExceeded",
      "msg": "attenuation would exceed MAX_DEPTH"
    },
    {
      "code": 6005,
      "name": "notASubset",
      "msg": "child cap/expiry/allowlist is not a subset of the parent's"
    },
    {
      "code": 6006,
      "name": "unauthorized",
      "msg": "signer does not own this capability"
    },
    {
      "code": 6007,
      "name": "allowlistTooLarge",
      "msg": "allowlist exceeds MAX_ALLOWLIST_LEN"
    },
    {
      "code": 6008,
      "name": "unauthorizedCaller",
      "msg": "only leash-hook may record a spend"
    },
    {
      "code": 6009,
      "name": "delegatedCannotRedeem",
      "msg": "a delegated capability cannot redeem its budget; only spend it"
    },
    {
      "code": 6010,
      "name": "notAnAncestor",
      "msg": "signer's capability is not an ancestor of the target capability"
    },
    {
      "code": 6011,
      "name": "notAChild",
      "msg": "this capability is not a child of the given parent"
    },
    {
      "code": 6012,
      "name": "childStillLive",
      "msg": "child is still live; revoke it or wait for expiry before reclaiming"
    }
  ],
  "types": [
    {
      "name": "capability",
      "docs": [
        "One node in a capability tree. A root capability has `parent = Pubkey::default()`; an",
        "attenuated child has `parent = <parent Capability pubkey>`.",
        "",
        "Invariant (checked by the program, not by convention — see BUILD_PLAN.md §2/§3):",
        "spent + committed_to_children <= cap"
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "owner",
            "docs": [
              "Signer who may attenuate or revoke this specific node."
            ],
            "type": "pubkey"
          },
          {
            "name": "parent",
            "docs": [
              "`Pubkey::default()` (all-zero) for root capabilities, never a real Capability",
              "address otherwise. Deliberately NOT `Option<Pubkey>`: Borsh encodes `Option<T>`",
              "as a 1-byte tag plus 32 bytes only when `Some`, so its on-disk size — and every",
              "field after it — would shift depending on whether a capability has a parent.",
              "leash-hook's extra-account-meta resolution needs `parent` at a **fixed** byte",
              "offset to read it directly out of raw account data (see `PARENT_FIELD_OFFSET`),",
              "so it has to be a fixed-size `Pubkey`, not an `Option`. Found by working through",
              "the hook's account-resolution design, not by guessing — see BUILD_PLAN.md §5."
            ],
            "type": "pubkey"
          },
          {
            "name": "ancestors",
            "docs": [
              "`[immediate parent, grandparent, great-grandparent]` (up to `ANCESTOR_SLOTS` =",
              "`MAX_DEPTH`), each `Pubkey::default()` past this capability's actual depth.",
              "Deliberately a **direct copy on this account**, not something leash-hook chains",
              "together by reading each ancestor's own `parent` field one hop at a time — that",
              "approach was tried first (Week 4) and broke: a root capability's `parent` is the",
              "System Program placeholder address, which has zero account data, so trying to",
              "read a \"further parent\" *out of* that placeholder's data fails at the",
              "client-side resolution step before a transaction is even submitted. Storing the",
              "whole chain here means every ancestor is read from *this* capability's own data",
              "(always real, always populated), never from a potentially-empty placeholder.",
              "Populated by `attenuate`: `[parent, parent.ancestors[0], parent.ancestors[1]]`."
            ],
            "type": {
              "array": [
                "pubkey",
                3
              ]
            }
          },
          {
            "name": "tokenAccount",
            "docs": [
              "The Token-2022 token account holding this capability's spendable balance."
            ],
            "type": "pubkey"
          },
          {
            "name": "cap",
            "docs": [
              "Total this capability may ever spend (cumulative, not a rolling window)."
            ],
            "type": "u64"
          },
          {
            "name": "spent",
            "docs": [
              "Cumulative amount spent so far via the transfer hook's spend path."
            ],
            "type": "u64"
          },
          {
            "name": "committedToChildren",
            "docs": [
              "Sum of `cap` handed to attenuated children. Not spendable by this node directly."
            ],
            "type": "u64"
          },
          {
            "name": "expiry",
            "docs": [
              "Unix timestamp. No spend may execute after this."
            ],
            "type": "i64"
          },
          {
            "name": "allowlist",
            "docs": [
              "Flat allowlist of destinations this capability (and, for now, its children) may",
              "pay. MVP: equality-or-subset check only, no arbitrary narrowing logic yet."
            ],
            "type": {
              "vec": "pubkey"
            }
          },
          {
            "name": "revoked",
            "docs": [
              "Set by `revoke`. The hook must also check every ancestor's flag, up to MAX_DEPTH."
            ],
            "type": "bool"
          },
          {
            "name": "depth",
            "docs": [
              "0 for a root capability; capped at MAX_DEPTH."
            ],
            "type": "u8"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    }
  ],
  "constants": [
    {
      "name": "authoritySeed",
      "docs": [
        "Single shared PDA used as both the wrapped mint's mint authority (issue mints,",
        "redeem burns don't need it, but a future re-mint might) and the vault's token-account",
        "authority (redeem withdraws real USDC from the vault, signed by this PDA). One PDA",
        "instead of two — nothing here needs them to be separate for the MVP."
      ],
      "type": "string",
      "value": "\"authority\""
    },
    {
      "name": "capabilitySeed",
      "docs": [
        "A Capability PDA is `[CAPABILITY_SEED, its own token account]` — *not* keyed on the",
        "owner. Keying on the owner is what limited each owner to a single capability forever",
        "(docs/ROADMAP.md 0.3): a second `issue` collided with the first PDA. The nonce that",
        "makes several capabilities possible lives in the token account's seeds below, and the",
        "capability inherits the distinction by being derived from that address.",
        "",
        "This indirection is not stylistic. leash-hook must re-derive \"the Capability for this",
        "transfer\" from **one fixed seed formula**, registered into the mint's",
        "`ExtraAccountMetaList` at deployment and resolvable only from accounts the transfer",
        "already carries — it cannot be handed a client-chosen nonce. The source token account",
        "*is* one of those accounts (base account 0), so folding the nonce into its address",
        "puts the disambiguator somewhere the hook can already reach."
      ],
      "type": "string",
      "value": "\"capability\""
    },
    {
      "name": "tokenAccountSeed",
      "docs": [
        "Seeds a capability's own wrapped-token account: `[TOKEN_ACCOUNT_SEED, owner, nonce]`,",
        "nonce as little-endian `u64`. One per capability, rather than the single ATA per",
        "(owner, mint) this used to rely on — an ATA is unique per owner, so it could not",
        "represent more than one capability. See CAPABILITY_SEED above for why the nonce goes",
        "here and not on the capability itself."
      ],
      "type": "string",
      "value": "\"capability-token\""
    },
    {
      "name": "vaultSeed",
      "docs": [
        "Seeds the vault holding a deployment's real deposited asset:",
        "`[VAULT_SEED, wrapped_mint]`. One vault per wrapped mint, created by",
        "`initialize_vault`.",
        "",
        "This constant existed unused for most of the project's life, and that was the bug",
        "(docs/ROADMAP.md 0.11): the vault was a client-generated keypair account, so no",
        "derivation tied it to anything and `issue`/`redeem` accepted whichever one the caller",
        "passed. Keying it on the wrapped mint is what makes \"which vault backs this mint\" a",
        "question the program can answer for itself instead of taking on trust."
      ],
      "type": "string",
      "value": "\"vault\""
    }
  ]
};
