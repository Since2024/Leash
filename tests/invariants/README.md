# Invariant / fuzz tests

One test per line in docs/BUILD_PLAN.md §8, each proving a violation attempt fails:

- spend exactly at cap succeeds; one unit over fails
- spend before expiry succeeds; one second after fails
- spend to an allowlisted destination succeeds; any other destination fails
- revoke a capability, then spend from it — fails
- revoke a *parent*, then spend from a *child* — fails
- attenuate a child with cap > parent's remaining budget — fails
- attenuate past MAX_DEPTH — fails

Mandatory before touching the CLI or SDK (docs/BUILD_PLAN.md §8). A bug here is, per the
build plan, a total-loss bug — treat this directory as gating, not optional.
