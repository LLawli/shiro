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

Three node kinds, and the kind alone decides what an invocation means:

- **menu**: has children, has nothing to execute. Naming it lists its children.
- **item**: a leaf carrying a recipe. Naming it runs the recipe.
- **action**: a leaf carrying one command. Naming it runs the command.

An item is something that becomes present: it has a `check` that answers whether
it is there, hooks that put it there, and a way back. An action is something
that happens: locking the screen, rebooting, taking a screenshot, cycling the
wallpaper. Nothing about it is ever "installed", nothing about it can be undone,
and a `check` on it would be answering a question nobody asked.

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

An action is declared with `[[action]]`, and carries one hook:

```toml
[[action]]
path        = "system.lock"
title       = "Lock the screen"
description = "Locks now; the shell asks for the password on return"
icon        = "system-lock-screen"
privilege   = "user"

[action.hooks]
run = "qs -c jibril ipc call lock lock"
```

It takes `path`, `title`, `description`, `order`, `hidden`, `mechanism`,
`privilege` and the presentation fields below, which is everything an item takes
except the parts about being installed. **`check`, `rollback`, `uninstall`,
`pre_mutates` and `[item.permissions]` are not fields of an action**, so a
declaration that sets one is refused by the parser, naming the key, rather than
carrying a field that means nothing. That is the same mechanism that keeps
`privilege` off a `check`: the shape of the declaration is what says so.

A list is declared with `[[list]]`, and its children come from a command:

```toml
[[list]]
path        = "theme.pick"
title       = "Theme"
description = "Pick among the themes installed"
privilege   = "user"

[list.entries]
command = "kuuhaku-themes --json"
ttl     = "5s"

[list.hooks]
run = '''kuuhaku-theme set "$SHIRO_ENTRY"'''
```

Half of a desktop menu is lists that only exist at run time: the wallpapers
actually on disk, the boxes that exist, the Flatpak applications installed, the
Bluetooth devices already paired. Written in a front end, they are the half of
the tree the catalog stops describing, and the menu stops growing by editing
TOML.

**The generator prints identities, and the catalog declares what runs.** This is
the whole shape of the feature. The generator answers "what is there", the
`run` hook is declared beside it in the same layer, and the chosen entry reaches
that hook as `SHIRO_ENTRY`. A generator printing nodes with hooks of their own
would move the declaration of what runs out of the catalog and into whatever the
command happened to print, and a generator in the user layer could then hand
itself `privilege = "system"`.

What it prints is a JSON array, one object per entry:

```json
[
  {"id": "nord", "title": "Nord"},
  {"id": "gruv", "title": "Gruvbox", "icon": "G", "value": "/themes/gruvbox"}
]
```

| Field | Meaning |
| --- | --- |
| `id` | Required. What the user types, so it is a path segment and obeys the segment rule below. |
| `title` | Required. |
| `description`, `icon`, `keywords` | Optional, meaning exactly what they mean on a declared node. |
| `value` | Optional. What the hook receives in `SHIRO_ENTRY`. The `id` when absent, which is the common case: a wallpaper is chosen as `foto-2024` and set by its absolute path, and neither string can do the other's job. |

An unknown key, a missing `id`, an `id` that is not a segment, an `id` printed
twice or an empty title is refused, loudly, rather than drawn. What the
generator prints is catalog data arriving late, and it is held to the rules
catalog data is held to.

**`mechanism`, `privilege`, `confirm`, `destructive`, `interactive` and
`keep_open` are declared once, on the list.** Every entry carries them, because
one hook runs them all. `icon` and `keywords` on the list describe the list
itself, the way they do for a menu; an entry's own come from the generator.

**Navigating a list is navigating a menu.** `shiro theme pick` lists what the
generator prints, and `shiro theme pick nord` runs the hook for that entry. A
segment the generator does not offer is refused the same way a path that stops
matching is, with what was available where it stopped. That check is what makes
an entry a choice out of a list rather than a string handed to a hook.

A list may not declare children of its own: the segment after it names an entry,
so a declared node under it is one nothing can navigate to.

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
| `icon` | every kind | A Nerd Font glyph or an XDG icon name. shiro does not interpret it, and does not care which it is. |
| `keywords` | every kind | Search terms beyond the title, so that "wifi" finds "Rede". |
| `confirm` | item, action, list | The question to ask before running. Its presence is what asks; its value is the wording. |
| `destructive` | item, action, list | Style it as such. Not the same as `confirm`, and neither implies the other. |
| `interactive` | item, action, list | The recipe needs a terminal: it prompts, or its progress is the point. |
| `keep_open` | item, action, list | After running, the menu stays where it is. Right for "next wallpaper", wrong for "reboot". |

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

Eight hooks on an item. Every one of them is optional except as noted.

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

An action has one, and it is mandatory:

| Hook | Role |
| --- | --- |
| `run` | Does the thing. An action has one phase, so it has one hook. |

A list declares two commands, and both are hooks like any other:

| Hook | Role |
| --- | --- |
| `entries.command` | Prints the entries. |
| `run` | Runs for the chosen entry, which arrives as `SHIRO_ENTRY`. |

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
  `app`; it holds together as the profile file the registry would write from it,
  declares the block belonging to its own backend, and, for a `flatpak` one, is
  written in words and values the permission vocabulary has, none of which is
  one that can never be granted (section 9). This is the one rule the validator
  does not answer itself: it hands the declaration to the permission module and
  reports the sentences it gets back, so the closed lists live in one place;
- `keywords` holds no empty entry and `confirm`, where declared, is not empty;
- a list declares a non-empty `entries.command`, and nothing is declared as a
  child of a list, whose children come from its generator.

The rules about `{ file = ... }` apply to an action's `run`, and to both
commands of a list, exactly as they do to any hook of a recipe. The rules about
rollback do not apply to an action or a list at all, because neither declares
anything they could be about.

Three rules from this section are enforced before validation and so never appear
as findings. A malformed path is refused by the loader. A hook carrying anything
beyond `file` is refused by the parser, which is the only place that can name
the stray key, and that is what keeps `privilege = "system"` off a `check`: a
hook has no `privilege` field to set. A `ttl` that is not a whole number and one
of `ms`, `s`, `m`, `h` is refused by the parser too, naming the key and the
line, rather than loading and meaning nothing.

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

### A generator is a `check` that answers with a list

`entries.command` is invoked unbidden, while a menu is drawing, exactly as a
`check` is, so the same three rules hold word for word: **no side effects, never
elevated, always bounded.** It is given no stdin, its stderr is inherited so
that a failure explains itself, and it runs under a timeout of five seconds,
overridable with `SHIRO_LIST_TIMEOUT` in whole seconds. Exceeding it, failing,
or printing something that is not the contract is an error (exit 1), not an
empty menu: a list that quietly draws nothing is indistinguishable from a
machine with nothing on it.

The difference from a `check` is that an answer may be kept. `ttl` on
`[list.entries]` says for how long, and its absence means the generator runs on
every listing. What is cached is what the generator printed, under
`$XDG_CACHE_HOME/shiro/entries/`, keyed by the node, its layer and the command
itself, so that editing a generator asks a different question rather than
invalidating an answer. `SHIRO_LIST_CACHE=0` turns caching off for one
invocation; exactly `0`, and nothing else.

The cache exists because the binary is spawned once per navigation step, so a
front end redrawing a level on every keystroke would otherwise respawn the
generator on every keystroke. It is not state in the sense of section 7: losing
it costs one process spawn, and nothing reads it to decide anything about the
machine.

### `uninstall` is derived when absent

If a recipe declares `uninstall`, that is what runs. If it does not, the engine
runs `roll-post`, `roll-install`, `roll-pre`, in that order, skipping the ones
that are absent. Most recipes remove with exactly the same command they roll
back with, and writing it twice is duplication that drifts apart in practice.
Declaring `uninstall` explicitly is the escape hatch for the cases where clean
removal genuinely differs from undoing a broken transaction.

## 5. Execution model

### Order

An action runs its `run` hook and stops. There is no gate in front of it, since
it declares no `check`, and nothing behind it, since it declares no rollback.
An entry chosen out of a list runs exactly like one, because that is what it is:
the list's `run` hook, with `SHIRO_ENTRY` set.
`--force`, `--uninstall` and `--keep-partial` are refused on an action rather
than ignored: there is nothing for them to act on, and a flag that is silently
dropped is a flag whose user believes it did something.

A recipe runs in four steps:

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
| `SHIRO_ENTRY` | The entry chosen out of a list: its `value`, or its `id` where the generator declared none. Absent for everything else, so a hook can tell "not given one" from "given an empty one". |

`SHIRO_RECIPE_DIR` is absent, rather than empty, for an item from the built-in
layer, which has no directory on disk: `cd "$SHIRO_RECIPE_DIR"` should fail
rather than land in the current directory.

`--dry-run` prints the exact command each hook would run, resolved, without
executing it. A tool whose job is to run arbitrary scripts as root owes the
user a way to read them first. Nothing that changes anything runs under it, not
even `check`. A list's generator is the exception, and it has to be: the entry a
dry run describes is the entry the generator printed, so refusing to ask it
would mean printing a guess about what `SHIRO_ENTRY` would hold.

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

**A level says which kind it is.** `kind` at the top of a listing is `menu` or
`list`, and a child that is a list carries `"kind": "list"`. Both are navigated
into the same way, so a consumer that treats `menu` as somewhere to go treats
`list` the same; what the distinction buys is knowing that the level behind it
was generated, and that it can change between two draws without the catalog
changing at all.

**An entry is listed as an action.** It carries the path it is reached by
(`theme.pick.nord`), `"kind": "action"`, whatever the generator said about
presentation, and what the list declared about running. It carries no `status`,
for the reason any action carries none, and no `value`: what the hook is given
is between the catalog and the hook.

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

**An action carries no `status` at all**, rather than `unknown`. The field
answers "is this installed", and that question does not arise for "reboot": a
front end reading `unknown` there would draw a verb as a package whose presence
could not be determined. A menu carries none either, for the same reason.

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
`removed`, `ok` (an action ran), `rolled-back` (the transaction failed and
everything was undone), `partial` (it failed and only the failed phase was
undone), `failed`, `rollback-failed` or `dry-run`.

An action emits the same two objects as any recipe, with `run` as the phase and
`ok` as the outcome. It is deliberately not `installed`: nothing is present now
that was absent before, which is the whole difference between the two kinds.

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

A list's `ttl` writes a file under `$XDG_CACHE_HOME/shiro/entries/`, and that is
not a second copy of any truth: it is what a generator printed, kept for as long
as the catalog said it may be reused. Deleting it costs one process spawn, and
nothing consults it to decide whether anything is installed.

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
are written in flatpak's own vocabulary, and the list is closed:

| Word | Flag |
| --- | --- |
| `network`, `ipc` | `--share` / `--unshare` |
| `socket=<name>` | `--socket` / `--nosocket` |
| `device=<name>` | `--device` / `--nodevice` |
| `feature=<name>` | `--allow` / `--disallow` |
| `filesystem=<path>` | `--filesystem` / `--nofilesystem` |
| `bus=<name>` | `--talk-name` / `--no-talk-name` |

The values are closed too, against what flatpak documents: sockets are `x11`,
`wayland`, `fallback-x11`, `pulseaudio`, `system-bus`, `session-bus`,
`ssh-auth`, `pcsc`, `cups`, `gpg-agent`, `inherit-wayland-socket`; devices are
`dri`, `input`, `usb`, `kvm`, `shm`, `all`; features are `devel`, `multiarch`,
`bluetooth`. A word or a value outside the lists is refused with the list rather
than forwarded, because a tool that passes through strings it does not
understand cannot be audited, and because the lists are what the deny below is
built from. Every invocation prints the `flatpak` command it is about to run.

**`apply` denies by default.** It emits every class flatpak has, denied by
name, and then what the profile allows. A manifest is not a promise: the
application that asked for a screen and a download directory in one release
asks for the session bus in the next, and an override file that lists only what
one profile took away grants whatever the next manifest adds. The profile is
therefore the whole of what the application may do, rather than a diff against
whatever it currently asks for.

The order inside the invocation is not cosmetic: the last mention of a key
wins, so the baseline is emitted first and the profile's allows come after.
`--nodevice=dri --device=dri` grants dri, and the reverse denies it.

The filesystem is the one class that does not need enumerating.
`--nofilesystem=host:reset` is documented as ignoring every filesystem
permission inherited from the manifest and from the override file, which is
exactly the deny-by-default the other classes have to spell out value by value.

**The session bus is not denied name by name, and does not need to be.** With
`--nosocket=session-bus`, flatpak proxies the bus rather than handing it over,
and its default policy already limits an application to its own name,
`org.freedesktop.DBus` and `org.freedesktop.portal.*`. Denying every name is
not expressible anyway: flatpak refuses `--no-talk-name=*`, and denying a
prefix broadly enough to matter (`org.freedesktop.*`) takes the portals with
it, which is how a confined application opens a file at all.

**Three things can never be granted**, in a profile or at the terminal, and are
denied on every `apply`:

| Refused | Why |
| --- | --- |
| `bus=org.freedesktop.Flatpak` | It is `flatpak-spawn --host`: arbitrary execution outside the sandbox. |
| `socket=session-bus`, `socket=system-bus` | The bus unfiltered, which is every name on it at once, including the one above. |
| `filesystem=` anything inside a flatpak installation | `~/.local/share/flatpak`, `/var/lib/flatpak`: the override files themselves, so the application could rewrite its own permissions. |

There is deliberately no escape hatch for these, because a flag that turns the
list off is the flag every recipe would copy. `apply` also denies `~/.var/app`,
which is every other application's data.

**`catalog validate` refuses the same words `apply` refuses.** A recipe's
`[item.permissions]` is checked against this vocabulary where the catalog is
checked, rather than only where it is applied: `apply` runs inside the recipe's
`post`, on the machine of whoever installed, and under `rollback = "atomic"` a
failure there takes the installation with it. The validator does not learn the
vocabulary to do it. It hands the declaration over and prints what comes back,
which is what keeps the closed lists in one place and the permission module the
only part of shiro that knows a mechanism.

What this does not do is make `filesystem=home` or `filesystem=host` safe. Both
reach the override directory by containing it, and both stay declarable,
because a profile that cannot express what a browser needs is a profile people
work around. The denial of the installation directories is emitted alongside
them as depth, not as a guarantee: how flatpak resolves a narrow deny against a
wide allow is flatpak's rule, not shiro's.

**`apply` emits one invocation carrying every side**, never one per side.
`flatpak override` merges into a file it keeps, and a deny that resets
(`filesystem=host:reset`) drops what is already in that file, so a second
invocation discards what the first one wrote.

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
