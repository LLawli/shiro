# Project rules for agents

Read [docs/architecture.md](docs/architecture.md) for the contract and
[docs/decisions.md](docs/decisions.md) for why it is shaped that way. The rules
below are the ones that are easy to miss and expensive to miss.

## The engine must never learn a mechanism

**No Flatpak, podman, distrobox, rpm-ostree or Quadlet knowledge goes into
`src/`.** The `mechanism` field of a recipe is a label for grouping and
display; the engine does not branch on it, ever.

Concretely, any of these is a violation:

- a `match` or `if` on `mechanism` anywhere in the engine;
- a hook the engine synthesizes because it "knows" what Flatpak needs;
- a validation rule that only makes sense for one mechanism;
- special-casing an error message by mechanism.

**Why:** the mechanisms are the part that changes, and they change on someone
else's schedule. Kept in recipes, a change is a TOML edit that any layer can
ship today. Moved into the engine, the same change is a release, and every
layer's curation becomes coupled to the engine version. This is the boundary
the whole design rests on, and it erodes one locally-reasonable exception at a
time.

If a mechanism genuinely cannot be expressed as hooks, that is a finding about
the hook model. Fix the hook model, in `docs/architecture.md`, in the same
commit.

## Changing the command surface or the schema means changing three things

Any change to the catalog schema, the hook set, the JSON payload or the native
command list requires, **in the same commit**:

1. the implementation;
2. `docs/architecture.md`, which is the reference both recipe authors and front
   end authors read;
3. the catalog validator, so the new rule is enforced rather than documented.

A schema rule that lives only in prose is a rule that gets violated by the
first recipe written after it.

If the change alters the `--json` payload in a way a consumer could notice,
bump `schema` and say so in `CHANGELOG.md`. The Vicinae extension releases
separately, so an unversioned break surfaces on a user's machine, not in CI.

## `check` has three hard rules

`check` is the only hook invoked in bulk, unbidden, while a menu is drawing.

1. **No side effects.** It answers a question.
2. **Never elevated.** Drawing a menu must never produce a password prompt. An
   item whose presence is only visible as root reports `unknown`.
3. **Always bounded.** Every invocation runs under a timeout and yields
   `timeout` rather than hanging.

The validator enforces what it can (no `privilege = "system"` on `check`). The
rest is review: a `check` that writes a cache file, or that calls a mechanism
with a subcommand that happens to create a directory, passes every automated
gate and breaks a menu.

## Startup time is a feature, not a benchmark

The binary is spawned once per navigation step. Nothing may be loaded, parsed,
resolved or spawned that the current invocation does not need. In particular:

- do not parse layers that cannot contribute to the requested path;
- do not run `check` on anything the caller did not ask about;
- keep the children's checks parallel and bounded.

This is the entire reason the project is compiled rather than shell. A change
that makes a cold `jibril install code --json` measurably slower needs to
justify itself against that.

## A recipe that mutates in `pre` must declare `roll-pre`

Expressed as `pre_mutates = true` on the item. The validator rejects the
combination of `pre_mutates` and a missing `roll-pre`. A `pre` that adds a
Flatpak remote or creates a box and then fails a later phase leaves debris that
nothing will ever clean up.

## Decisions go in `docs/decisions.md`, in the commit that makes them

Including the rejected alternatives and the reason. The file exists so that
nobody, human or model, re-proposes something that was already investigated and
turned down. If a decision recorded there stops holding, change the file in the
same commit that changes the behavior, rather than leaving two contradicting
sources.

## Writing style

- **Never use the em dash.** Use a comma, parentheses, a colon or a separate
  sentence. It reads as machine-written and this project's prose is read
  closely.
- Code, messages and `docs/` are English. The README exists in both English and
  Portuguese, and both are updated together.

## Commits

Conventional commits, one logical change each, SSH-signed. No AI co-author
trailers, no generated-with footers.
