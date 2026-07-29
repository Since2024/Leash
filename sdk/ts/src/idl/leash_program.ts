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
                "path": "childOwner"
              }
            ]
          }
        },
        {
          "name": "wrappedMint",
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
            "via CPI, owned by `child_owner` directly (same bearer-object model as `issue`)."
          ],
          "writable": true
        },
        {
          "name": "token2022Program"
        },
        {
          "name": "associatedTokenProgram"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
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
      "name": "issue",
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
          "name": "vault",
          "docs": [
            "time. Its authority is `program_authority` below."
          ],
          "writable": true
        },
        {
          "name": "wrappedMint",
          "docs": [
            "configured at deployment time). Mutable because minting increases supply."
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
                "path": "principal"
              }
            ]
          }
        },
        {
          "name": "capabilityTokenAccount",
          "docs": [
            "created here via CPI to the associated-token-account program. Owned by",
            "`principal` directly — the capability is a bearer object the holder controls,",
            "not something leash-program gates access to (BUILD_PLAN.md §0)."
          ],
          "writable": true
        },
        {
          "name": "tokenProgram"
        },
        {
          "name": "token2022Program"
        },
        {
          "name": "associatedTokenProgram"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
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
          "name": "capability",
          "docs": [
            "legitimately not exist (a merchant who holds received units and no capability) —",
            "the handler distinguishes the two by inspecting owner/data rather than trusting",
            "the caller, so this cannot be typed as `Account<Capability>` or `Option<_>`.",
            "Mutable because a root's redemption writes `cap` back down.",
            "",
            "NOTE: this derivation assumes one capability per owner. When docs/ROADMAP.md 0.3",
            "lands, capabilities are keyed off their own token account instead, and this",
            "should derive from `holder_wrapped_account` — at which point the association",
            "becomes exact rather than \"the capability this holder happens to have.\""
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
                "path": "holder"
              }
            ]
          }
        },
        {
          "name": "holderWrappedAccount",
          "writable": true
        },
        {
          "name": "wrappedMint",
          "writable": true
        },
        {
          "name": "vault",
          "writable": true
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
          "name": "tokenProgram"
        },
        {
          "name": "token2022Program"
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
      "type": "string",
      "value": "\"capability\""
    },
    {
      "name": "vaultSeed",
      "type": "string",
      "value": "\"vault\""
    }
  ]
};
