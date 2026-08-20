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
| `shiro doctor` | Environment report: which layers loaded, what each contributed, and which `mechanism` labels the catalog uses. |
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
| built-in | embedded in the binary at build time from `catalog/` | the shiro repo: the root nodes, and no recipes |
| image | `/usr/share/shiro/catalog/` | the distro image (kuuhaku-os) |
| machine | `/etc/shiro/catalog/` | the machine administrator |
| user | `$XDG_DATA_HOME/shiro/catalog/` (default `~/.local/share/shiro/catalog/`) | the user, per user |

The built-in layer holds the root nodes (`install`, `update`, `theme`) so that a
freshly built binary has a command surface with a stable shape, and so a layer
above only has to declare children. It holds no recipes: those live in the layer
that can ship a change without a shiro release. The image layer is how a distro
ships its own opinions. The machine layer
earns its place on bootc specifically: `/usr` is the image and `/etc` belongs
to the machine, so `/etc` is the only writable place an administrator can add
a recipe without rebuilding an image.

**Merge is per node path.** A layer contributes new nodes and replaces existing
ones wholesale: there is no field-level merge, because a half-overridden recipe
is impossible to reason about when a hook fails. To suppress an inherited node
without replacing it, declare it with `hidden = true`.

Within one layer, two files declaring the same path is an error rather than a
race: the winner would depend on the order the filesystem happened to return.

`shiro catalog sources` exists to answer "why is this item behaving like
that", which is otherwise the worst class of bug an override system produces.

**`SHIRO_ROOT` reroots the two system layers**, so that `/usr/share` and `/etc`
are read from under it instead. It exists so the loader can be exercised, and
another machine's catalog inspected, without writing to either directory. The
user layer needs nothing of the sort: `XDG_DATA_HOME` already moves it.

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

**A path is segments joined by dots, and a segment is what the user types.** A
segment matches `[a-z0-9][a-z0-9_-]*`: it is lowercase, and it never needs
quoting, because a segment that has to be quoted is a segment nobody can type
out of a menu. A malformed path is refused at load, not reported as a finding:
the tree cannot hold it.

**`order` sorts a menu's children, and the title breaks ties.** A child with no
`order` sorts after every child that has one. A catalog assembled from four
layers is partially ordered most of the time, and the ones that declared an
order should stay where they were put.

**An unknown key is an error, everywhere.** Catalogs are hand-written, and
silently ignoring `mechnism = "flatpak"` produces an item whose author believes
they set a field they did not.

**A hook is a shell command or a script file.** A string is passed to `sh -c`,
which covers the one-liner and, as a multi-line string, the short inline
script. A `{ file = "..." }` table points at a script relative to the directory
of the TOML file that declared it. There is deliberately no third form.

**`mechanism` is a label, not a dispatch key.** It exists for grouping,
filtering and reporting, and it is what a front end shows as a badge. The
engine never branches on it. The day it does, the boundary in section 5 is
broken.

**Presentation is catalog data.** A graphical front end needs more than a title
and a description, and every one of these varies per node and per layer, which
is exactly what makes it a declaration rather than something a front end can
infer from a path. A front end that guesses an icon from the path breaks the
moment a layer adds a node.

| Field | Where | Meaning |
| --- | --- | --- |
| `icon` | menu, item | A Nerd Font glyph or an XDG icon name. shiro does not interpret it, and does not care which it is. |
| `keywords` | menu, item | Search terms beyond the title, so that "wifi" finds "Rede". |
| `confirm` | item | The question to ask before running. Its presence is what asks; its value is the wording. |
| `destructive` | item | Style it as such. Not the same as `confirm`, and neither implies the other. |
| `interactive` | item | The recipe needs a terminal: it prompts, or its progress is the point. |
| `keep_open` | item | After running, the menu stays where it is. Right for "next wallpaper", wrong for "reboot". |

None of them changes what the engine does, with the single exception of
`confirm`, which is asked at a terminal (section 5). `interactive` in particular
has no engine behavior at all, since a hook already inherits stdin: it exists so
that a front end knows that `flatpak install` is a progress stream while
`distrobox enter` is a shell, which it has no other way to tell.

`confirm` is the question and not a boolean, so that the wording belongs to the
layer that declared the item, which is also the layer that knows what language
the menu speaks. There is no boolean form, for the same reason a hook has no
third form.

**Declaration fields are snake, hook keys are kebab.** `keep_open` and
`pre_mutates` on one side, `roll-pre` and `roll-install` on the other. The TOML
key and the JSON key are the same string, so there is no translation table
between the format a recipe is written in and the payload a front end reads.

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
- a root node may not claim a name in the native namespace;
- every node below the root needs a declared parent, and that parent must be a
  menu: an item has no children, and an orphan is a node nobody can navigate to;
- a `{ file = ... }` hook is relative to its own recipe: it may not be absolute,
  may not climb out with `..`, and must exist. This is checked for every hook,
  not only `check`;
- a `{ file = ... }` hook may not appear in the built-in layer at all, which is
  embedded in the binary and has no directory on disk. Inline the script;
- `[item.permissions]` names `run` or `flatpak` as its backend, and a non-empty
  `app`.

Two rules from this section are enforced before validation and so never appear
as findings. A malformed path is refused by the loader. A hook carrying anything
beyond `file` is refused by the parser, which is the only place that can name
the stray key, and that is what keeps `privilege = "system"` off a `check`: a
hook has no `privilege` field to set.

### `check` is a special hook

It is the only hook invoked in bulk, on items the user has not chosen, while a
menu is being drawn. Three rules follow from that, and all three are hard:

1. **No side effects.** It answers a question and changes nothing.
2. **Never elevated.** A menu drawing itself must not produce a password
   prompt. An item whose presence can only be determined as root reports
   `unknown`, which is an honest answer and a cheap one.
3. **Bounded.** Every `check` runs under a timeout; exceeding it yields
   `timeout`, not a hang. A front end drawing a list gets an answer for every
   child in bounded time or a clear reason it did not. The timeout is three
   seconds, overridable with `SHIRO_CHECK_TIMEOUT` in whole seconds. A check
   also gets no stdin and has its output discarded: a hook that stops to ask a
   question times out rather than blocking the menu that called it.

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

### Removing

`--uninstall` on the item's path removes it: `shiro install code vs-code
--uninstall`. It is a flag rather than a verb because the path is the item's
address and the verbs belong to the catalog, so a native `uninstall` command
would read `shiro uninstall install code vs-code`.

The gate runs in the other direction: if `check` reports absent, the removal is
refused and `--force` overrides it. A removal has no rollback of its own. It
stops at the first hook that fails and reports what is left, because "undo the
undo" is not a state any recipe describes.

### Confirming

An item that declares `confirm` (section 3) does not run until the question has
been answered. There is one rule and no exemption that depends on the output
mode: **`--yes` answers it, and a terminal may answer it live.**

- at a terminal, without `--json`, shiro asks on stderr and reads the answer;
  anything that is not `y` or `yes` is a no;
- everywhere else, an unanswered question is a refusal (exit 3) naming the
  question and `--yes`.

A front end reads the question out of the listing, draws its own dialog and
passes `--yes`. That flag is therefore the single signal that the question has
been answered, by a human directly or by a program on a human's behalf.

`--force` does not answer it. It skips the `check` gate and nothing else: the
gate is about the state of the system and the question is about the intent of
the user, and one is not evidence of the other.

### Flags

| Flag | Effect |
| --- | --- |
| `--json` | Structured output, section 6. The only flag the reporting commands take. |
| `--force` | Skip the `check` gate, and nothing else. |
| `--yes` | Answer an item's `confirm` in advance. |
| `--dry-run` | Print the exact command each hook would run, resolved, and run none of them. |
| `--keep-partial` | Downgrade `atomic` to `phase` for this invocation. |
| `--uninstall` | Remove instead of install. |

### Exit codes

A front end acts on these, so they are contract:

| Code | Meaning |
| --- | --- |
| 0 | It worked. |
| 1 | A hook failed, or the catalog could not be loaded. Whatever the rollback policy asked for was done. |
| 2 | The invocation was wrong: unknown flag, a path that stops matching, arguments after an item. |
| 3 | shiro declined and nothing happened: already installed, or not installed and asked to remove. |
| 4 | A rollback hook failed. The system is in a state neither the user nor the recipe intended. |

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

Elevation is `sudo`, and `SHIRO_SUDO` replaces it (`run0`, `doas`, `pkexec`, or
`sudo` with arguments). The single prompt is a `sudo true` before any hook runs,
which fills the credential cache that then covers the transaction. A tool
without such a cache will prompt again per hook, which is a property of that
tool rather than of shiro.

**`SHIRO_SUDO_PROBE=0` drops the probe.** Under an elevator that keeps no cache
the probe fills nothing: it is one password dialog, and every hook still asks
again afterwards. A graphical front end wants `pkexec` precisely because it
routes to the desktop's polkit agent instead of needing a terminal, and two
dialogs for one menu click is what pushes it back to `sudo` in a terminal. With
the probe dropped, the first hook is what prompts, so a recipe with one hook
costs one dialog.

What that gives up is the clean refusal. With the probe, a cancelled password
leaves the transaction unstarted and shiro says so and exits. Without it, the
same cancellation arrives as the first hook failing, and whatever the rollback
policy asks for runs on top of it. Exactly `0` drops the probe; any other value,
and the absence of the variable, keeps it.

An elevated hook carries its environment inside the script, as `export`
statements ahead of the body, because sudo resets the environment and a default
sudoers refuses `VAR=value` in front of a command. `--dry-run` prints that form,
so what is read is what would run.

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
| `SHIRO_DRY_RUN` | `1` in the environment `--dry-run` prints, `0` when the hook runs. |

`SHIRO_RECIPE_DIR` is absent, rather than empty, for an item from the built-in
layer, which has no directory on disk: `cd "$SHIRO_RECIPE_DIR"` should fail
rather than land in the current directory.

`--dry-run` prints the exact command each hook would run, resolved, without
executing it. A tool whose job is to run arbitrary scripts as root owes the
user a way to read them first. Nothing runs under it, not even `check`.

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
      "icon": "com.visualstudio.code",
      "keywords": ["vscode", "editor"],
      "mechanism": "flatpak",
      "privilege": "user",
      "confirm": "Install Visual Studio Code?",
      "interactive": true,
      "layer": "image",
      "status": "installed"
    }
  ]
}
```

The root is a menu like any other, and it is the one no catalog declares: it
lists with `"path": ""` and `"title": "shiro"`.

**A field with no value is omitted, never emitted as null**, so that a
consumer's "is it absent" and "is it empty" are the same check. A child that is
a menu therefore carries no `mechanism`, `privilege` or `status`.

**A boolean is emitted only when it is true**, by the same rule: absent and
`false` are one answer, and a menu of two hundred children should not pay for a
key per child that says nothing. `destructive`, `interactive` and `keep_open`
appear when set and are absent otherwise. An empty `keywords` is omitted rather
than emitted as `[]`.

The native commands answer in the same shape when given `--json`: a `schema`
field, a `kind` naming the payload (`doctor`, `version`, `validation`,
`sources`), and the body. They are versioned by the same field for the same
reason.

`status` is one of `installed`, `absent`, `unknown` (no `check` declared, or it
would require elevation) or `timeout`. A front end uses it to decide which
action to offer, and then **invokes that action explicitly**: shiro has no
toggle command, because a menu acting on a stale status would uninstall
something the user meant to install.

Running a recipe emits one object per phase transition, so a front end can show
progress on a long install rather than a spinner. The final object carries the
outcome and the exit code.

```json
{"schema":1,"kind":"phase","item":"install.code.vs-code","phase":"install","state":"start"}
{"schema":1,"kind":"phase","item":"install.code.vs-code","phase":"install","state":"ok","exit":0}
{"schema":1,"kind":"result","item":"install.code.vs-code","outcome":"installed","exit":0}
```

`state` is `start`, `ok`, `failed` or `dry-run`. `outcome` is `installed`,
`removed`, `rolled-back` (the transaction failed and everything was undone),
`partial` (it failed and only the failed phase was undone), `failed`,
`rollback-failed` or `dry-run`.

**A hook's own output goes to stderr under `--json`**, so a long install still
shows its progress while stdout stays a clean stream of objects. Capturing it
instead would hold it back until the phase ended, which is the opposite of what
the per-phase reporting is for.

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

| Command | Effect |
| --- | --- |
| `shiro perms flatpak <app>` | Print the current overrides (`show` is the default). |
| `shiro perms flatpak <app> allow <perm>…` | Grant. |
| `shiro perms flatpak <app> deny <perm>…` | Revoke. |
| `shiro perms flatpak <app> reset` | Drop every override. |
| `shiro perms flatpak <app> apply` | Apply what the registered profile declares. |
| `shiro perms run <app>` | Print the profile in effect and the exact bwrap invocation `shiro run` would use. |

`--system` acts on the system-wide override instead of the user's. Permissions
are written in flatpak's own vocabulary, and the list is closed: `network`,
`ipc`, `filesystem=<path>`, `device=<name>`, `bus=<name>`. A word outside it is
refused with the list rather than forwarded, because a tool that passes through
strings it does not understand cannot be audited either. Every invocation prints
the `flatpak` command it is about to run.

**`apply` emits one invocation carrying both sides**, never one per side.
`flatpak override` merges into a file it keeps, and a deny that resets
(`filesystem=host:reset`) drops what is already in that file, so a second
invocation discards what the first one wrote. Within one invocation flatpak
resolves the whole set, in either order.

`shiro perms run <app>` is the audit surface for the bwrap side, which has no
`--show` of its own: it prints what the profile grants and the invocation that
grants it, so the two can be read against each other.

### `shiro run <app> [args…]`

For native applications, which have no permission mechanism of their own. It
resolves a profile, builds the bwrap invocation from it, and execs the program.

**The fallback is close to nothing.** An application with no profile still
runs, under a sandbox that grants the minimum a process needs to start and
nothing else: no network, no home, no devices, no session bus. The application
will fail to do most of what it wants, and that is the intended outcome, so the
failure has to be legible: falling back prints a warning to stderr naming the
profile that was looked for and where a profile could be placed.

The base every sandbox starts from, profile or not:

- every namespace unshared, and the network only back if a profile says so;
- `/usr` and `/etc` bound read only, plus the merged-usr symlinks, which is what
  makes a process start at all: the loader, fonts, locale, CA certificates;
- a private `/proc`, a minimal `/dev`, a tmpfs over `/tmp`;
- **a tmpfs over the home directory**, so an application that writes there works
  and writes nowhere the user can see;
- an emptied environment, rebuilt with `PATH`, `HOME` and the terminal's locale,
  so that tokens and paths from the launching shell do not leak in;
- `--die-with-parent` and `--new-session`: nothing outlives the launcher, and
  nothing can push characters back into the terminal that started it.

A program installed outside `/usr` is not in the sandbox at all, so shiro warns
about that specifically rather than leaving bwrap to report a missing file.

### The profile format

```toml
# ~/.local/share/shiro/profiles/brave.toml
backend = "run"
app     = "brave"

[run]
command    = "/usr/bin/brave-browser"   # without it, the name is resolved on PATH
network    = true
share      = ["wayland", "pipewire"]    # wayland | x11 | pipewire | pulse | session-bus
devices    = ["dri"]                    # short name or absolute path
read-only  = ["/etc/fonts"]
read-write = ["~/Downloads"]

[run.env]
MOZ_ENABLE_WAYLAND = "1"
```

A `flatpak` profile carries the overrides instead, in the same vocabulary the
command takes:

```toml
backend = "flatpak"
app     = "com.brave.Browser"

[flatpak]
allow = ["filesystem=~/Downloads", "device=dri"]
deny  = ["filesystem=home"]
```

`~` is the user's home and nothing else is expanded: a profile is read by
someone deciding what an application may reach, so a path in it should mean what
it looks like. **The file name and the `app` key must agree**, and a profile
where they disagree is refused rather than guessed about, since guessing wrong
means confining the wrong program.

Declaring a profile does not apply it. `shiro run` reads a `run` profile at
launch; a `flatpak` profile is applied when something calls `shiro perms flatpak
<app> apply`, which is a recipe's `post` doing its own work.

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

**When it happens:** after `install` and before `post`, so that a `post`
generating whatever calls `shiro run` finds the profile already there. It is
reported as a phase named `permissions`, because a front end showing progress
should not have a silent step in the middle of it.

**Rollback:** the registry write is the one mutation the engine performs on its
own initiative, so the engine owns undoing it. It takes its place in the
sequence like a completed phase, and is reverted with the rest of the
transaction under the item's rollback policy, without the recipe declaring
anything. Reverting means putting back exactly what was there, which for a
profile that did not exist means removing the file rather than leaving an empty
one.

### Open: one binary or two

`shiro run` sits in the hottest path in the system, executed on every launch of
every sandboxed application, where the engine's catalog machinery is dead
weight. A separate, smaller `shiro-run` binary would start faster at the cost
of a second artifact to build, ship and version.

Deferred until it can be measured: build both, compare cold start on the launch
path, and split only if the difference is perceptible when an application
starts. Deciding now would be guessing, and the guess is cheap to defer because
the wrappers a recipe generates call a command name, not a code path.
