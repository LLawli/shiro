# Architecture

This document defines the contract: the catalog format, how a command path
resolves against it, how a recipe executes, and what the structured output
looks like. It is the reference a recipe author and a front end author both
read. If behavior and this file disagree, one of them is a bug.

## 1. The catalog is the command surface

There is no hardcoded list of subcommands for the curated part of shiro. The
catalog declares a tree; the arguments after the binary name are a path into
that tree.

```sh
shiro install                 # node "install": a menu, so it lists children
shiro install code            # node "install/code": a menu, lists children
shiro install code vs-code    # node "install/code/vs-code": an item, executes
```

Two node kinds, and the kind alone decides what an invocation means:

- **menu**: has children, has nothing to execute. Naming it lists its children.
- **item**: a leaf carrying a recipe. Naming it runs the recipe.

There is no `list` verb, and therefore no reserved word that a catalog author
can collide with. A menu with no arguments left to consume *is* the list.

The tree is not limited to `install`. `update`, `theme` and anything else the
distro wants to expose are root nodes in the catalog like any other, which is
what lets kuuhaku-os add a whole command group without a shiro release.

Alongside the tree there are **native commands**, implemented in Rust, for the
things that require seeing the engine's own state:

| Command | Purpose |
| --- | --- |
| `shiro doctor` | Environment report: which layers loaded, which mechanisms are available on this host. |
| `shiro catalog validate` | Parses every layer, enforces the schema rules in section 4, exits non-zero on violation. |
| `shiro catalog sources` | Prints each loaded layer and which nodes it contributed or overrode. |
| `shiro version` | Version, and the digest of the merged catalog. |
| `shiro perms <backend> …` | The permissions module, section 9. |
| `shiro run <app> [args]` | Launches a native application under bwrap, section 9. |

Native commands live under a namespace that the catalog may not claim. The
validator enforces that: a catalog defining a root node named `doctor` fails to
load rather than being silently shadowed. Shadowing is worse than a conflict,
because the failure appears as a command that quietly does the wrong thing.

## 2. Catalog layers

Four layers, lowest precedence first:

| Layer | Location | Owner |
| --- | --- | --- |
| built-in | embedded in the binary at build time from `catalog/` | the shiro repo: the base curation |
| image | `/usr/share/shiro/catalog/` | the distro image (kuuhaku-os) |
| machine | `/etc/shiro/catalog/` | the machine administrator |
| user | `$XDG_DATA_HOME/shiro/catalog/` (default `~/.local/share/shiro/catalog/`) | the user, per user |

The built-in layer exists so that a freshly built binary is useful with no
files on disk, and so the base curation is versioned with the engine that runs
it. The image layer is how a distro ships its own opinions. The machine layer
earns its place on bootc specifically: `/usr` is the image and `/etc` belongs
to the machine, so `/etc` is the only writable place an administrator can add
a recipe without rebuilding an image.

**Merge is per node path.** A layer contributes new nodes and replaces existing
ones wholesale: there is no field-level merge, because a half-overridden recipe
is impossible to reason about when a hook fails. To suppress an inherited node
without replacing it, declare it with `hidden = true`.

`shiro catalog sources` exists to answer "why is this item behaving like
that", which is otherwise the worst class of bug an override system produces.

## 3. Catalog format

One or more `.toml` files per layer, discovered recursively. File names are
irrelevant: the path comes from the declarations, not the filesystem, so a
layer can split or merge files freely without moving nodes.

```toml
# /usr/share/shiro/catalog/code.toml

[[menu]]
path  = "install.code"
title = "Code editors"
order = 20

[[item]]
path        = "install.code.vs-code"
title       = "Visual Studio Code"
description = "Microsoft's editor, Flatpak build"
mechanism   = "flatpak"     # descriptive only, see below
privilege   = "user"        # user | system
rollback    = "atomic"      # atomic | phase | none

[item.hooks]
check        = "flatpak info --user com.visualstudio.code"
install      = "flatpak install --user -y flathub com.visualstudio.code"
post         = { file = "scripts/vs-code-post.sh" }
roll-install = "flatpak uninstall --user -y com.visualstudio.code"
```

An item may also declare `[item.permissions]`, which the engine records into
the profile registry and does nothing else with. See section 9.

**A hook is a shell command or a script file.** A string is passed to `sh -c`,
which covers the one-liner and, as a multi-line string, the short inline
script. A `{ file = "..." }` table points at a script relative to the directory
of the TOML file that declared it. There is deliberately no third form.

**`mechanism` is a label, not a dispatch key.** It exists for grouping,
filtering and reporting, and it is what a front end shows as a badge. The
engine never branches on it. The day it does, the boundary in section 5 is
broken.

## 4. Recipe hooks

Eight hooks. Every one of them is optional except as noted.

| Hook | Role |
| --- | --- |
| `check` | Reports whether the item is present. Exit 0 means installed. |
| `pre` | Verifies compatibility and prepares. May mutate. |
| `install` | Performs the installation. This is what "installed" means. |
| `post` | Configures: dotfiles, service enablement, permissions. |
| `roll-post` | Undoes `post`. |
| `roll-install` | Undoes the installation. |
| `roll-pre` | Undoes `pre`. |
| `uninstall` | Removes an item that is installed and intact. |

Schema rules the validator enforces:

- an item with a `pre` that mutates must declare `roll-pre`, expressed as
  `pre_mutates = true` on the item; the validator rejects the combination of
  `pre_mutates` and a missing `roll-pre`;
- an item with `install` must declare `roll-install` unless `rollback = "none"`;
- `check` may not be a `{ file = ... }` pointing outside its layer, and may not
  declare `privilege = "system"` (see below).

### `check` is a special hook

It is the only hook invoked in bulk, on items the user has not chosen, while a
menu is being drawn. Three rules follow from that, and all three are hard:

1. **No side effects.** It answers a question and changes nothing.
2. **Never elevated.** A menu drawing itself must not produce a password
   prompt. An item whose presence can only be determined as root reports
   `unknown`, which is an honest answer and a cheap one.
3. **Bounded.** Every `check` runs under a timeout; exceeding it yields
   `timeout`, not a hang. A front end drawing a list gets an answer for every
   child in bounded time or a clear reason it did not.

Batch invocation runs the children's checks in parallel. This is not an
optimization detail, it is the contract: 200 serial process spawns per menu
open would defeat the reason the engine is compiled at all.

### `uninstall` is derived when absent

If a recipe declares `uninstall`, that is what runs. If it does not, the engine
runs `roll-post`, `roll-install`, `roll-pre`, in that order, skipping the ones
that are absent. Most recipes remove with exactly the same command they roll
back with, and writing it twice is duplication that drifts apart in practice.
Declaring `uninstall` explicitly is the escape hatch for the cases where clean
removal genuinely differs from undoing a broken transaction.

## 5. Execution model

### Order

`check` (gate) → `pre` → `install` → `post`.

`check` runs first on the chosen item. If it reports installed, `shiro install
<item>` **refuses and explains**, and suggests `--force`. Recipes are written by
hand and idempotence cannot be proven by a validator, so re-running one by
accident is a real way to break a working system. `--force` skips the gate and
nothing else.

### Rollback

The item declares its policy in `rollback`:

| Value | Behavior on failure |
| --- | --- |
| `atomic` (default) | Undo everything already done, in reverse order, back to the initial state. |
| `phase` | Undo only the phase that failed, then stop and report the partial state. |
| `none` | Undo nothing. Report and exit. |

`atomic` is the default because the interactive case dominates: the user picked
an item from a menu and pressed enter, and the useful guarantee is that it
either worked or nothing changed. `phase` exists for recipes where a partial
install is genuinely more useful than none, such as a heavy download that
succeeded while configuration failed.

`--keep-partial` downgrades `atomic` to `phase` for a single invocation, for
debugging a recipe without paying the reinstall.

**A failing rollback is a distinct outcome**, not a second failure of the same
kind. The system is now in a state neither the user nor the recipe author
intended, and the report says so explicitly, names the hook that failed and
exits with a dedicated code.

### Privilege

Declared per item, never per hook. `privilege = "user"` runs everything
unelevated; `privilege = "system"` elevates the whole recipe. The engine
determines the requirement before running anything and prompts once, up front.

Per-hook privilege was the more precise design and was rejected: it produces a
password prompt in the middle of a transaction, which is exactly when the user
has walked away, and a timed-out prompt turns into a rollback of work that had
already succeeded. `check` is exempt because it is never elevated at all.

### Environment

Every hook runs with a defined environment, so a recipe never has to guess
where it is:

| Variable | Meaning |
| --- | --- |
| `SHIRO_ITEM` | Full node path of the item being acted on. |
| `SHIRO_PHASE` | The hook currently running. |
| `SHIRO_RECIPE_DIR` | Directory of the TOML file that declared the item, for locating sibling scripts and assets. |
| `SHIRO_LAYER` | Which layer the item came from. |
| `SHIRO_DRY_RUN` | `1` when running under `--dry-run`. |

`--dry-run` prints the exact command each hook would run, resolved, without
executing it. A tool whose job is to run arbitrary scripts as root owes the
user a way to read them first.

## 6. Structured output

`--json` on any invocation. This is the contract the Vicinae extension is
written against, so it is versioned: the payload carries a `schema` field and a
breaking change to it is a breaking change to shiro.

Listing a menu:

```json
{
  "schema": 1,
  "kind": "menu",
  "path": "install.code",
  "title": "Code editors",
  "children": [
    {
      "path": "install.code.vs-code",
      "kind": "item",
      "title": "Visual Studio Code",
      "description": "Microsoft's editor, Flatpak build",
      "mechanism": "flatpak",
      "privilege": "user",
      "layer": "image",
      "status": "installed"
    }
  ]
}
```

`status` is one of `installed`, `absent`, `unknown` (no `check` declared, or it
would require elevation) or `timeout`. A front end uses it to decide which
action to offer, and then **invokes that action explicitly**: shiro has no
toggle command, because a menu acting on a stale status would uninstall
something the user meant to install.

Running a recipe emits one object per phase transition, so a front end can show
progress on a long install rather than a spinner. The final object carries the
outcome and the exit code.

## 7. State

shiro keeps no database of what it installed. The truth about whether Visual
Studio Code is present is held by Flatpak, and a second copy of that truth
would be a copy that drifts. `check` asks the system, every time.

The cost is that `check` is a process spawn per item, which is precisely why it
is bounded, parallel, and never elevated.

## 8. Repository layout

```
Cargo.toml
src/                    # the engine: parse, resolve, execute, report
src/perms/              # the permissions module, section 9
catalog/                # base curation, embedded into the binary at build time
profiles/               # base bwrap profiles, embedded alongside the catalog
docs/                   # this file, vision.md, decisions.md
packaging/install.sh    # verified installer, mirroring sora's
tests/
.github/workflows/      # CI and release
```

## 9. The permissions module

The engine is forbidden from knowing mechanisms (section 5, and the first rule
in `CLAUDE.md`). This module is the single, delimited exception: knowing about
Flatpak overrides and bwrap *is* its purpose. The boundary that keeps the
exception from spreading is that the module is only reachable through its own
native commands and through one declarative field, and the recipe executor
never calls into it.

Two surfaces, and they do not overlap.

### `shiro perms <backend> …`

For applications that already have a permission mechanism. Today that means
Flatpak, where shiro drives `flatpak override` rather than inventing storage of
its own.

**The backend is always explicit**: `shiro perms flatpak <app> …` and
`shiro perms run <app> …`. Never inferred from the application name. A
permission tool that guesses which mechanism it is talking to cannot be
audited, and being auditable is most of the value here. The verbosity is the
feature.

### `shiro run <app> [args…]`

For native applications, which have no permission mechanism of their own. It
resolves a profile, builds the bwrap invocation from it, and execs the program.

**The fallback is close to nothing.** An application with no profile still
runs, under a sandbox that grants the minimum a process needs to start and
nothing else: no network, no home, no devices, no session bus. The application
will fail to do most of what it wants, and that is the intended outcome, so the
failure has to be legible: falling back prints a warning to stderr naming the
profile that was looked for and where a profile could be placed.

Failing open (running unconfined when a profile is missing) was never
considered. A sandbox that silently disappears is worse than no sandbox,
because the user believes it is there.

### Profiles

Profiles are TOML, one per application, in four layers with the same precedence
as the catalog:

| Layer | Location |
| --- | --- |
| built-in | embedded in the binary from `profiles/` |
| image | `/usr/share/shiro/profiles/` |
| machine | `/etc/shiro/profiles/` |
| user | `$XDG_DATA_HOME/shiro/profiles/` |

A higher layer replaces a profile wholesale, exactly as with catalog nodes, and
for the same reason: a half-overridden sandbox is a sandbox nobody can reason
about. **The user layer may loosen**, not only tighten. This is a single-user
workstation tool, and a user who cannot grant their own browser a directory
will edit a `.desktop` file to bypass `shiro run` entirely, which is a strictly
worse outcome than letting them write the profile.

### `[item.permissions]` in a recipe

A recipe may declare the profile for what it installs:

```toml
[item.permissions]
backend = "run"          # run | flatpak
app     = "brave"
# backend-specific body follows
```

**The engine records this and does nothing else with it.** It writes the entry
into the profile registry, and that is the entire extent of the interaction:

- it does not invoke bwrap or flatpak;
- it does not generate a wrapper script;
- it does not generate or edit a `.desktop` file.

Generating the executable or the desktop entry that calls `shiro run` is the
recipe's `post`, like any other configuration work. This keeps the split
honest: the declaration is data the module owns, and the wiring is a recipe
concern the engine does not understand.

**Which layer receives the write** follows the item's `privilege`: a `system`
item writes to `/etc/shiro/profiles/` and a `user` item to
`$XDG_DATA_HOME/shiro/profiles/`. Never `/usr/share`, which belongs to the
image and is read-only on bootc.

**Rollback:** the registry write is the one mutation the engine performs on its
own initiative, so the engine owns undoing it. It is reverted with the rest of
the transaction under the item's rollback policy, without the recipe declaring
anything.

### Open: one binary or two

`shiro run` sits in the hottest path in the system, executed on every launch of
every sandboxed application, where the engine's catalog machinery is dead
weight. A separate, smaller `shiro-run` binary would start faster at the cost
of a second artifact to build, ship and version.

Deferred until it can be measured: build both, compare cold start on the launch
path, and split only if the difference is perceptible when an application
starts. Deciding now would be guessing, and the guess is cheap to defer because
the wrappers a recipe generates call a command name, not a code path.
