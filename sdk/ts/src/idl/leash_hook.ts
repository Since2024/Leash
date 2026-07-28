/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/leash_hook.json`.
 */
export type LeashHook = {
  "address": "9WPQUY6zVRwVZ3eUsDF1aNESWAyZwL8GwKpzd2C66xtS",
  "metadata": {
    "name": "leashHook",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Leash: Token-2022 TransferHook enforcement (cap/expiry/allowlist/revoked). See docs/BUILD_PLAN.md."
  },
  "docs": [
    "Real spend-path enforcement (Week 3/4, BUILD_PLAN.md §4/§7). Checks cap/expiry/",
    "allowlist/revoked, and now the full ancestor chain up to MAX_DEPTH (Week 3 checked",
    "only one level). See docs/BUILD_PLAN.md §5 \"Week 1 spike results\" and the doc",
    "comments below for what changed since the Week 1 placeholder."
  ],
  "instructions": [
    {
      "name": "initializeExtraAccountMetaList",
      "docs": [
        "D4, resolved for real: registers the source's own Capability PDA and its parent,",
        "derived dynamically per-transfer instead of Week 1's fixed placeholder.",
        "",
        "Every Capability (root from `issue`, or child from `attenuate`) is seeded as",
        "`[CAPABILITY_SEED, owner]` — one fixed formula regardless of root/child status,",
        "which is exactly what makes deriving it here possible (see attenuate.rs's doc",
        "comment on why the child seed scheme had to change to match).",
        "",
        "Extra accounts registered, in order (indices 5-10, after the 5 base accounts):",
        "5. leash_program's own ID — needed as the \"owning program\" anchor for the PDA",
        "derivation below (Capability accounts belong to leash-program, not this",
        "hook), and reused as the CPI target account in `spend_logic`.",
        "6. source_capability — external PDA, `[CAPABILITY_SEED, owner]` under",
        "leash-program. Writable: `record_spend` (via CPI) mutates it.",
        "7-9. ancestor1 / ancestor2 / ancestor3 — each read directly out of",
        "**source_capability's own** `ancestors` array (`ANCESTORS_FIELD_OFFSET`),",
        "not chained through each ancestor's own `parent` field one hop at a time.",
        "The chained version was tried first and broke: a root capability's `parent`",
        "resolves to the System Program's placeholder address, which has zero",
        "account data, so trying to read a \"further parent\" out of *that* data fails",
        "at the client-side resolution step (confirmed by hitting exactly this",
        "failure, not by anticipating it — see docs/BUILD_PLAN.md §5 Week 4 results).",
        "Storing the full ancestor chain directly on each Capability (`state.rs`)",
        "avoids ever reading from a potentially-empty account. Whichever slots a",
        "transfer's capability doesn't actually have resolve to Pubkey::default()",
        "(harmless here, since they're read from real, always-populated data) —",
        "`spend_logic` only checks as many as `capability.depth` says exist.",
        "10. hook_authority — this program's own PDA, used purely as the CPI signer for",
        "`record_spend`. Not derived from anything transfer-specific."
      ],
      "discriminator": [
        92,
        197,
        174,
        197,
        41,
        124,
        19,
        3
      ],
      "accounts": [
        {
          "name": "payer",
          "writable": true,
          "signer": true
        },
        {
          "name": "extraAccountMetaList",
          "docs": [
            "Transfer Hook Interface spec — not an Anchor-typed account."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  120,
                  116,
                  114,
                  97,
                  45,
                  97,
                  99,
                  99,
                  111,
                  117,
                  110,
                  116,
                  45,
                  109,
                  101,
                  116,
                  97,
                  115
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ]
          }
        },
        {
          "name": "mint"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": []
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "revoked",
      "msg": "capability has been revoked"
    },
    {
      "code": 6001,
      "name": "parentRevoked",
      "msg": "this capability's parent has been revoked"
    },
    {
      "code": 6002,
      "name": "expired",
      "msg": "capability has expired"
    },
    {
      "code": 6003,
      "name": "notAllowlisted",
      "msg": "destination is not on this capability's allowlist"
    },
    {
      "code": 6004,
      "name": "capExceeded",
      "msg": "spend would exceed this capability's remaining budget"
    }
  ]
};
