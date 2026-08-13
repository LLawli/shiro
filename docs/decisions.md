# Design decisions

The record of what was decided, why, and what was rejected, so that future
contributors (human or LLM) do not relitigate it from scratch. If a decision
here stops holding, change this file in the same commit that changes the
behavior.

## Implementation language: Rust

The binary is ephemeral and invoked once per menu navigation step. Drawing a
node means: start the process, parse the catalog, resolve a path, spawn the
children's `check` hooks, print. That happens on every keystroke-level
interaction in the Vicinae extension, so process startup is a user-visible
cost, not a benchmark curiosity. An interpreted runtime pays it every time.

This is the opposite conclusion from [sora](https://github.com/LLawli/sora),
and deliberately so. sora's hot path is a shell hook that must not spawn an
interpreter at all, which forces shell. shiro's hot path is a process that
must start fast and then fan out, which forbids one.

Rust over Go on a secondary point: no runtime, a fully static-ish binary that
drops into an image with no library concerns, and a stronger type-level grip on
the catalog schema, which is the part most likely to grow accidental
complexity.

## The catalog is the command surface, not a data file the CLI reads

Subcommands are not written in Rust. The TOML declares a tree of menus and
items, and the argument path is a path into that tree.

This is what makes the curation the product. Adding a whole command group is a
TOML file; a distro can extend the surface without a shiro release; a user can
add a private group in their home directory. The alternative, a fixed CLI that
reads a list of installables, makes every structural idea wait on a release.

The cost is accepted knowingly: the help text, the completions and the
validation all have to be generated from data rather than written, and errors
about a malformed catalog have to be as good as errors about a wrong command.

## Menus list themselves; there is no `list` verb

`shiro install code` lists, because a menu has nothing to execute. A `list`
suffix would have made `list` a reserved word that a catalog author could
collide with, and the collision would surface as a curated item that is
unreachable for reasons the author cannot see. A prefix verb or a `--list` flag
would have avoided the collision at the cost of the natural typing order.

Native commands keep a reserved namespace regardless (see architecture.md
section 1), but that set is small, fixed and validated loudly.

## No argument-parsing crate

Argument handling is hand-written: split argv into a native command or a path
into the catalog, and hand the rest to whoever runs it.

A declarative parser describes a fixed surface, and shiro's surface is not fixed
and is not known until the catalog is loaded. What clap or a peer would actually
describe is the five native commands, which is the part that needs the least
help, while the part that needs real work (a good error for a path that stops
matching halfway, help text and completions generated from data) it cannot do at
all. Adding it would buy a dependency in the hot path and leave both jobs
undone.

Revisit if the flag surface grows past what is comfortable to read in one
function. The catalog path itself never becomes a parser's problem.

## The engine never learns a mechanism

There is no Flatpak code in the engine, no podman code, no distrobox code.
`mechanism` in a recipe is a label used for grouping and display; the engine
does not branch on it.

**Why:** the mechanisms are the part that changes. Flatpak grows a flag, a new
Quadlet syntax lands, sora changes a subcommand. If those live in recipes, each
is a data edit that any layer can ship. If they live in the engine, each is a
release, and the base curation and the distro's curation get coupled to the
engine version.

This is the boundary that the design most depends on and is the easiest to
erode. A pull request that adds `if mechanism == "flatpak"` to the engine is
rejected on principle even when it is locally the smaller change.

**The one exception is the permissions module**, whose entire purpose is to
know about Flatpak overrides and bwrap. It is delimited by being reachable only
through its own native commands and one declarative field, and by the recipe
executor never calling into it. The rule above is therefore about the executor,
precisely: no branch on `mechanism`, anywhere, for any reason.

## `doctor` reports the labels a catalog uses, and does not probe the host

`shiro doctor` says which layers loaded, what each contributed, and how many
items carry each `mechanism` label. It does not say whether Flatpak is installed.

Reporting availability would require knowing what a label means, which is the
one thing the engine may not know. The near miss is worse than the obvious
violation: checking for an executable whose name matches the label reads as
neutral, and quietly makes `mechanism` a dispatch key with one user.

**Open:** if "can this host run these recipes" turns out to be worth answering,
the way to answer it is a declared probe, in the catalog, next to the label it
belongs to. That adds schema, so it waits for a real need rather than a
hypothetical one.

## The built-in layer holds TOML and nothing else

A `{ file = "..." }` hook in the built-in layer is a validation error, and the
fix is to inline the script. The layer is embedded in the binary at build time,
so a path relative to "the directory of the TOML that declared it" points at a
directory that does not exist on the host.

Rejected embedding the scripts too and extracting them at run time: it puts a
temporary directory and a write in the launch path of every recipe, to serve a
base curation that does not need it. If the base curation ever grows a script
long enough to be worth a file, this decision is what should be revisited.

## No state database

The engine records nothing about what it installed. `check` asks the system.

A local database would make `--installed` listings cheap and removal
bookkeeping easy, and would be wrong every time the user touched Flatpak
directly, which they will. Two sources of truth for "is this present" is a bug
generator, and the bug it generates is uninstalling something the tool
mis-remembers.

The price is real and is paid on purpose: one process spawn per item when
drawing a menu, mitigated by running them in parallel under a timeout, never
elevated.

## Rollback policy is declared per recipe, defaulting to atomic

Atomic is right for the interactive case that dominates: the user chose an item
from a menu, and the useful guarantee is "it worked, or nothing changed".

It is not right for everything, which is why `phase` exists. A recipe that
downloads gigabytes and then fails to write a config file should not throw the
download away. Making that a per-recipe decision keeps the default honest
without pretending one policy fits both.

`none` exists for recipes that are pure mutations of user configuration, where
"undo" has no meaning the engine could implement correctly.

## `uninstall` is derived from the rollback hooks when absent

For most recipes, undoing an installation and removing an installed thing are
the same command. Requiring both to be written means writing the same string
twice, and the second copy is the one nobody updates.

Rejected the inverse (deriving `roll-install` from `uninstall`) because it is
the weaker direction: a rollback runs against a system in a partial, possibly
broken state, and a removal command written for the intact case can fail there
in ways that are hard to diagnose. The derivation should go from the more
defensive form to the less defensive one, not the other way around.

## The main hook is named `install`, not `in`

`in` was the first name and was changed before any catalog existed. It is a
reserved word in Rust, which forces a rename at the serde boundary for no
benefit, and it reads badly next to its own rollback: `roll-in` is fine, `in`
alone is not. The hook set is now symmetric: `pre` / `install` / `post`, each
with a matching `roll-*`.

The overlap with the `install` root node in the catalog is contextual and
harmless: one is a hook key inside an item, the other is a path segment.

## Reinstalling refuses instead of re-running

`shiro install <item>` on an item that `check` reports as installed exits with
an error and suggests `--force`.

Recipes are hand-written shell. Idempotence cannot be validated, only claimed,
and a non-idempotent recipe re-run against a working install can leave the
system worse than either state. Refusing is the behavior that never destroys
anything; `--force` is one word for the case where the user knows better.

Rejected "re-run only `post`" as a default: a third behavior for the user to
remember, and a silent one, since the difference between "reinstalled" and
"reconfigured" would not be visible in the outcome.

## Removal is a flag on the item's path, not a verb

`shiro install code vs-code --uninstall`, not `shiro uninstall install code
vs-code`.

The path is the item's address, and the verbs in it belong to the catalog. A
native `uninstall` command would sit in front of a path that starts with
`install`, reserve another name the catalog cannot use, and read worse for every
root node that is not called `install`. The flag reads oddly next to `install`
and that is the whole of its cost, paid once per invocation by someone who
already knows what they are removing.

Rejected a toggle for the reason already recorded below: shiro never decides
between installing and removing from a status it just read.

## Elevation is sudo, and the prompt happens once, before anything runs

`sudo true` runs before the first hook, and the credential cache it fills covers
the rest of the transaction. `SHIRO_SUDO` replaces the tool for a machine that
prefers `run0`, `doas` or `pkexec`.

sudo is the default because of that cache, which is the mechanism that makes
"prompt once, up front" true rather than aspirational. `run0` is the more modern
answer on Fedora bootc and has no equivalent, so a two-phase recipe under it can
authenticate twice, which is exactly what deciding privilege per item was meant
to avoid. That is a property of the tool, and the variable is there for someone
who wants it anyway.

An elevated hook receives its environment as `export` statements inside the
script rather than as `VAR=value` in front of the command, because a default
sudoers refuses the latter. `--dry-run` prints the same form, so what a user
reads before authorising is what runs.

## Privilege is declared per item, not per hook

Per-hook privilege is more precise and was rejected. It puts an authentication
prompt in the middle of a running transaction, which is when the user has
looked away, and a prompt that times out turns into a rollback of work that had
already succeeded. One decision up front, prompting once or never, is the
behavior that survives a distracted user.

`check` is the exception in the other direction: it may never elevate at all,
because it runs unbidden while a menu draws.

## Cancelled: shiro as a separate, ambitious permissions project

The original plan had a fourth project named shiro: a general sandboxing layer
covering distrobox boxes, Flatpaks and native applications under bwrap, aiming
well past what firejail does. It was cancelled before a line was written, and
this project took its name.

**Why it was cancelled:** the scope was larger than the distro it was meant to
serve, and it was the last of the four to start, which is a combination that
predicts a project that never ships. Meanwhile the thing that actually needed
solving was narrow and concrete: the browser should run sandboxed even though
it is not a Flatpak, and Flatpak overrides should be manageable without
memorising the CLI.

**Why it landed here rather than staying separate and smaller:** installing a
tool and deciding what it may touch happen at the same moment. The point where
something new lands on the system is when its blast radius is decided, and the
only time anyone thinks about it. Keeping the two in one binary means a recipe
can declare a profile as part of describing the tool.

The cost is a real one and it is the reason this entry exists: the engine now
contains a subsystem that knows about specific mechanisms (Flatpak, bwrap),
which the engine is otherwise forbidden from doing. The boundary that keeps
that from spreading is documented in architecture.md and enforced by review,
not by structure. If it erodes, this decision was the wrong one.

**What was given up:** distrobox boxes are not covered. The original scope
included them, and the module ships without them because there is no bounded,
obvious mechanism there the way `flatpak override` and `bwrap` are bounded and
obvious. Revisit if a real need shows up.

## The engine records `[item.permissions]` and does nothing else with it

It writes the declaration into the profile registry. It does not invoke bwrap
or flatpak, does not generate a wrapper, does not touch a `.desktop` file. The
recipe's `post` generates whatever executable or desktop entry calls
`shiro run`.

Rejected "the engine does everything", which would have removed boilerplate
from every recipe: it puts wrapper generation and desktop-entry editing inside
the engine, where they become a mechanism the engine has to keep up with, and
`.desktop` conventions are exactly the kind of thing that drifts.

Rejected "the engine ignores the field, recipes call `shiro perms set` in
`post`", which is the purest version of the boundary: it makes the declaration
inert documentation that can silently disagree with what the recipe actually
does, and it puts a mutation outside the transaction, where the engine cannot
roll it back.

What is left is the smallest useful thing the engine can do with the field:
turn data into data. Because that write is the engine's own mutation rather
than a recipe's, the engine owns reverting it, and it participates in the
item's rollback policy with nothing declared.

## `shiro perms` always names its backend

`shiro perms flatpak <app>` and `shiro perms run <app>`, never
`shiro perms <app>` with the backend inferred.

Inferring would be friendlier and was rejected. This is a tool whose output the
user has to trust when reasoning about what an application can reach, and a
tool that silently picks a mechanism cannot be audited: the same command would
mean different things for two applications, with nothing on screen to say
which. The verbosity is the feature.

## The sandbox is built by adding to nothing

The base binds `/usr` and `/etc` read only, gives a private `/proc`, a minimal
`/dev`, a tmpfs over `/tmp` and a tmpfs over the home directory, empties the
environment, and unshares every namespace. A profile then adds network, devices,
desktop sockets and paths. Nothing is ever removed from a fuller starting point.

Starting from the host and subtracting was the alternative, and it fails the
only test that matters: a permission nobody thought to subtract stays granted,
and the mistake is invisible. Starting from nothing makes the same mistake
visible immediately, as an application that does not work.

`/etc` read only is the one concession, and it is what makes a process start at
all: the loader configuration, fonts, locale, CA certificates. It holds no user
data, and the files in it that are secrets are unreadable to the user the
sandbox runs as, inside it or outside.

The environment is emptied rather than filtered for the same reason: a filter
lists what to drop, and the token that leaks is the one nobody listed.

## The profile declares capabilities, not bwrap arguments

`network = true`, `share = ["wayland"]`, `read-write = ["~/Downloads"]`, rather
than the bwrap flags they become.

Passing flags straight through would have kept shiro out of the business of
knowing what bwrap options mean, but a profile is read by someone deciding what
an application may reach, and `--ro-bind-try /run/user/1000/wayland-0 …` is not
that sentence. The vocabulary is small and closed, and `shiro perms run <app>`
prints both the summary and the resulting invocation, so the translation is
checkable rather than trusted.

## Declaring a profile does not apply it

A `run` profile is read at launch by `shiro run`, and there is nothing to apply.
A `flatpak` profile is applied only when something calls `shiro perms flatpak
<app> apply`, which is a recipe's `post` doing its own work.

The engine recording a declaration and something changing on the system are kept
separate on purpose. It is the same boundary as rule 3 in `CLAUDE.md`: the write
is data, and every mutation past it has a command behind it that a user can see,
repeat and undo.

## Profiles layer like the catalog, and the user layer may loosen

Same four layers, same precedence, same wholesale replacement rather than
field-level merge, for the same reason: a half-overridden sandbox cannot be
reasoned about.

The user may loosen a profile, not only tighten it. A stricter design was
considered (system defines the ceiling, user may only restrict further) and
rejected as security theatre in this context: this is a single-user
workstation, the user is root, and a user who cannot grant their own browser
one directory will edit the `.desktop` file to bypass `shiro run` entirely.
That outcome is strictly worse, because it removes the sandbox rather than
adjusting it.

## The bwrap fallback grants almost nothing, and says so

An application with no profile runs under a sandbox with no network, no home,
no devices and no session bus. It will fail to do most of what it wants.

Failing open was never on the table: a sandbox that silently disappears when a
profile is missing is worse than no sandbox at all, because the user believes
it is there. The cost of failing closed is a confusing breakage, and that is
paid down by making the fallback loud: it warns on stderr, names the profile it
looked for, and says where one can be placed.

## Portable engine, opinionated curation

The engine assumes a POSIX shell and nothing else. The curation is written for
Fedora bootc and does not pretend otherwise: a layering module would be
`rpm-ostree` and would simply not apply elsewhere.

This split is what lets kuuhaku-os be the first consumer rather than the only
one, the same relationship sora already has with it. A host that lacks a
mechanism gets recipes that fail their `pre`, which is the correct outcome and
requires no special casing.

## Structured output is a versioned contract

`--json` carries a `schema` field. The Vicinae extension is a separate artifact
with its own release cadence, so the payload is an interface between two
independently versioned things, and interfaces that are not versioned break
silently at the worst moment.

## The catalog digest covers what nodes declare, not where they live

`shiro version` prints a short digest of the merged catalog: sixteen hex
characters of a sha256 over each node's declaration, in path order.

It deliberately does not cover the file paths the nodes came from, so that the
same catalog reached through a different mount point digests the same. It is a
"are these two machines running the same curation" answer, which is the question
that comes up when a recipe behaves differently on two hosts. It is not an
attestation, and the catalog is trusted input already: it runs shell.

## No toggle command

A front end that knows an item's status invokes `install` or `uninstall`
explicitly. shiro exposes no command that decides between them from a status
it just read, because that status can be stale by the time the user clicks, and
the failure mode is removing something the user meant to add.

## Distribution: private release tarball with a token

Same shape as sora (tarball, version and sha256 pinned in the consumer), but
the repository is private for now, so the kuuhaku-os build needs a read token
to fetch the release.

Rejected building from source inside the `Containerfile`: it would drag a Rust
toolchain into the image build for every rebuild, and it would lose the
version-and-digest pinning that makes the sora installation auditable. Opening
the repository at first release is the path that removes the token entirely,
and remains available.

## The release artifact is one static musl binary per architecture

A tag builds `x86_64-unknown-linux-musl` and publishes
`shiro-<version>-<target>.tar.gz` containing the binary alone, plus a `.sha256`
next to it. The installer verifies the checksum before it installs anything.

musl because the artifact's job is to drop into an image and work, and a glibc
build ties the binary to the libc of whatever built it, which is the one thing
an image-based host has no way to fix afterwards. The tarball holds only the
binary because the base curation is embedded at build time: there is nothing
else to ship yet, and inventing a layout now would have to be undone when
generated completions arrive.

Only x86_64 is built, because that is what kuuhaku-os targets. Another
architecture is a target in the release workflow and a case in the installer,
and the installer already fails on an unrecognised one with a message that says
to build from source rather than a confusing 404.

## English code and messages, bilingual README

Same as sora: the project was born in Portuguese, the audience is not. Code,
messages and `docs/` are English; the README exists in both languages.

## License: MIT

No dependency constraints of consequence, and the catalog schema and hook model
are worth more copied than protected.
