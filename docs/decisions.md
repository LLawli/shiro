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

## The curation does not ship inside the engine

The built-in layer holds the root nodes (`install`, `update`, `theme`) and no
recipes. The recipes live in the image layer, which for this ecosystem means
kuuhaku-os.

The alternative was tempting for one reason: a freshly cloned, freshly compiled
shiro would do something. It loses on the reason the whole design exists.
Mechanisms change on someone else's schedule, and a recipe embedded in the
binary turns "flatpak grew a flag" into a shiro release, which is exactly the
coupling the engine was shaped to avoid. It would also put Fedora-specific
curation inside a binary that assumes nothing beyond a POSIX shell.

What is kept is the small, stable part: the root nodes give a front end a
command surface with a fixed shape and consistent titles, and a layer above only
declares children under them. A root can still be replaced outright, since merge
is per node path.

The cost is that `shiro install` on a bare machine lists nothing, so it says so
and names the three directories a layer can be placed in. An empty menu that
explains itself is a different thing from an empty menu.

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

## An action is its own declaration, not an item with a flag

`[[action]]` alongside `[[menu]]` and `[[item]]`, rather than
`kind = "action"` on an item, which is how it was first proposed.

A desktop menu is mostly things that have no installed state: lock, reboot,
suspend, screenshot, cycle the wallpaper, restart the shell. Shaped as items,
they lie in four fields at once. `check` would have to answer a question they
do not raise, `rollback` describes an undo that does not exist, and a front end
receives `status: unknown`, which reads as "shiro could not determine whether
this is installed" and means nothing at all about rebooting.

**Rejected: a `kind` field on `[[item]]`.** It is one struct instead of two and
it costs four negative rules in the validator, each saying that a field which
exists means nothing in this case. Worse, a recipe that sets `check` on an
action then loads successfully and is only reported later, by a command nobody
is obliged to run. As its own declaration, `check` is simply not a field, so the
parser refuses it, names the key and the line, and does it at load. That is the
same mechanism that already keeps `privilege` off a `check`, recorded in
section 4 of architecture.md: the shape of the declaration is the rule.

The cost is real and is duplication: `MenuDecl`, `ItemDecl` and `ActionDecl`
repeat the fields they share, because `#[serde(flatten)]` silently disables
`deny_unknown_fields`, and that flag is what makes `mechnism = "flatpak"` an
error rather than a lost field. Given the choice between repeating six field
declarations and losing the rule that catches every typo in every catalog, the
repetition is the cheaper half by a wide margin.

The JSON is unaffected by any of this: `kind` is `"action"` there, which is what
a front end was going to be told either way.

## A generator prints entries; the hook that runs them stays in the catalog

`[[list]]` declares a command that prints identities (`id`, `title`, and the
presentation fields), and one `run` hook beside it. The chosen entry reaches
that hook as `SHIRO_ENTRY`, and nothing else about it is generated.

**Rejected: the generator prints children, hooks and all**, which is how the
request in issue #2 was written: a command emitting the same shape shiro emits,
spliced under the node. It is more flexible in one way that matters little,
heterogeneous children under one node, and it moves the declaration of what runs
out of the catalog. The property the whole design rests on is that the command
surface is data an administrator curates in layers; a generator printing hooks
makes it whatever the command printed on that particular run, and a generator in
the user layer could hand itself `privilege = "system"` for a hook the machine
layer never saw. Printing identities keeps the generator on the same footing as
a `check`: it answers a question about the machine and decides nothing.

The cost is that a list is homogeneous. Every entry runs the same hook with a
different subject, which is exactly what "pick a theme" and "enter a box" are,
and a node that needs two different verbs for its children is a menu with two
lists under it.

## An entry is checked against the generator before its hook runs

`shiro theme pick nord` asks the generator what it offers and refuses `nord` if
it is not there, with the same shape of message a path that stops matching gets.

Passing the segment straight through would save a spawn (often not even that,
since a `ttl` usually has the answer already) and would turn the entry into a
string the user hands to a hook rather than a choice out of a list. The refusal
is also the only thing that makes the front end's job and the terminal's job the
same job: a menu can only offer what the generator printed, and the terminal now
behaves the same way.

`id` is a path segment, under the same rule as every other segment, because it
is what a user types. `value` exists for the case where what is typed and what
the command needs cannot be the same string: a wallpaper is chosen as
`foto-2024` and set by an absolute path with spaces in it. Rejected: allowing an
arbitrary `id` and quoting it. A segment that needs quoting is a segment nobody
can type out of a menu, which is the rule the tree already keeps.

## A generated answer may be cached, and that is not a state database

`ttl` on `[list.entries]` keeps what the generator printed under
`$XDG_CACHE_HOME/shiro/entries/`, keyed by the node, its layer and the command.

Section 7 says shiro keeps no database of what it installed, and that still
holds: this file holds no truth about the machine, and nothing consults it to
decide whether something is present. It holds what one command printed, for as
long as the catalog said it may be reused, and deleting it costs one spawn.

It has to live in shiro rather than in the front end because the binary is
spawned once per navigation step. A panel redrawing a level on every keystroke
would respawn the generator on every keystroke, and the only place that knows
the answer may be reused is the declaration that said so.

**Rejected: no cache at all, and let the front end keep one.** Every front end
would then implement the same expiry, and the terminal, which has no front end,
would get none of it. Rejected too: caching by default with a built-in expiry.
A list of Bluetooth devices and a list of installed themes go stale at rates
that only their author knows, so the declaration carries it, and a list that
says nothing is not cached at all.

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

## A confirmation is refused rather than assumed, and `--json` is not a bypass

An item declaring `confirm` needs `--yes`, unless a terminal is there to answer
it live. Nothing about the output mode changes that.

The field exists so that "Really power off?" belongs to the item rather than to
a list of paths hardcoded in a front end. Once it is data, the terminal has to
honour it too, or the promise the whole design rests on breaks in the worst
direction: everything the menu can do, the terminal can do with the same words,
and the terminal would be the more dangerous of the two.

**Rejected: proceeding when there is no terminal to ask.** It is the friendlier
default and it is precisely the inversion above. A piped invocation would run
what the menu asks about, silently, and the property would hold only for the
case that was already safe.

**Rejected: `--json` exempting the caller.** It reads correct, since a front end
under `--json` has the question in the listing and draws its own dialog. It also
makes `--json` the shortest way to skip a confirmation, which is a bypass
sitting inside a flag that is otherwise about formatting. One flag means "the
question has been answered", and a front end passes it after its dialog like
anybody else.

**Rejected: `--force` answering it.** `--force` skips the `check` gate and
nothing else, which is already in the contract. The gate is about the state of
the system and the question is about the intent of the user; treating one as
evidence of the other is how a flag grows a second meaning nobody documented.

The cost: a script that drives a confirming item has to say `--yes`. That is the
intended cost, and it is discoverable, because the refusal names the flag.

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

## The probe can be dropped, and shiro never detects the elevator

`SHIRO_SUDO_PROBE=0` skips the `sudo true` that runs before the first hook.

It exists for `pkexec`, which is the elevator a graphical front end wants,
because it routes to the desktop's polkit agent rather than needing a terminal,
and which keeps no credential cache. The probe therefore authenticates nothing
under it: it is one password dialog, and the hooks still ask again. Two dialogs
for one menu click is enough to push a front end back to `sudo` in a terminal,
which is the outcome the polkit agent existed to avoid.

**Rejected: detecting `pkexec` and dropping the probe automatically.** It is the
same shape as the `:reset` special case rejected in the permissions module. It
reads as neutral and quietly turns the elevator's name into a dispatch key with
one user, after which the engine owes the same knowledge to `run0`, to `doas`
and to whatever comes next. The variable puts the decision where the knowledge
is, which is with whoever configured the elevator.

**Rejected: dropping the probe outright.** Under `sudo`, which is the default,
the probe is the entire mechanism behind "prompt once, up front": without it,
that promise becomes a property of how many hooks a recipe happens to declare.

The cost is real and is the reason this is opt-in rather than the default. With
the probe, a cancelled password leaves the transaction unstarted and shiro says
so. Without it, the same cancellation arrives as the first hook failing, and the
item's rollback policy runs on top of it. A front end that sets the variable is
trading a clean refusal for one dialog, knowingly.

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

## `perms flatpak apply` denies every class first, then grants the profile

`apply` emits `--unshare`, `--nosocket`, `--nodevice`, `--disallow` for every
value flatpak documents, plus `--nofilesystem=host:reset`, and only then what
the profile allows.

The reason is that a manifest is not a promise. Permissions in flatpak default
to what the application asks for, and an application asks again on every
update: the release that wanted a screen and a download directory wants the
session bus two months later, and an override file that lists only what a
profile took away in 2026 silently grants whatever 2027 adds. Under
deny-by-default the profile is the whole of what the application may do, and an
update that wants more has to be answered by editing the profile, which is a
file somebody reads.

The cost is real and is paid at the profile: an application now gets nothing it
did not ask for by name, `socket=wayland` included, so a profile that used to
be a short list of exceptions becomes a description of what the application
needs. That is the trade being made deliberately. A profile that grants nothing
produces an application that does not start, which is a visible failure, and
the alternative failure is invisible.

**Rejected: keeping the old shape and auditing manifests.** It puts the burden
where nobody looks, on noticing that an update changed its `Context` section.
**Rejected: a `sealed = false` escape on the profile**, for the application that
"needs everything": it is the flag every recipe copies, and a deny-by-default
with a documented way to turn it off is a deny-by-default nobody can rely on.

The order inside the invocation matters and is tested: the last mention of a
key wins, so `--nodevice=dri --device=dri` grants dri and the reverse denies it.
The baseline is emitted first for that reason, not for readability.

The closed value lists (11 sockets, 6 devices, 3 features) are what the baseline
is built from, which is why a value shiro does not know is refused rather than
forwarded: an unknown value is one the baseline never denied, so accepting it
would grant something that had no name in the deny.

## D-Bus is denied by socket, not by name

`apply` emits `--nosocket=session-bus` and `--nosocket=system-bus`, plus a short
list of names, and does not attempt to deny names in general.

It cannot: flatpak refuses `--no-talk-name=*` outright (`Invalid dbus name *`),
and prefixes broad enough to matter take the portals with them, since
`org.freedesktop.portal.*` is how a confined application opens a file. It also
does not need to. With the socket denied, flatpak proxies the bus instead of
handing it over, and the default policy already limits the application to its
own name, `org.freedesktop.DBus` and the portals. That is deny-by-default
already, written by flatpak.

What the short list adds is the names that would be granted by a manifest and
that are equivalent to leaving the sandbox: `org.freedesktop.Flatpak`, which is
`flatpak-spawn --host`, and `org.freedesktop.impl.portal.PermissionStore`,
which is where the portals keep their answers.

## What would undo a sandbox cannot be granted, and there is no flag for it

`bus=org.freedesktop.Flatpak`, the two bus sockets, and any `filesystem=` inside
a flatpak installation are refused wherever they are declared: in a profile, in
a recipe's `[item.permissions]`, and typed at the terminal. `apply` refuses the
whole profile rather than applying the rest of it.

Each grants the thing that lets an application undo its own confinement, which
makes granting it indistinguishable from not confining the application. A
sandbox with a hole in it is not most of a sandbox.

`filesystem=home` and `filesystem=host` are deliberately **not** on that list,
though both contain the override directory. A profile that cannot express what
a browser needs is a profile people route around, by editing the `.desktop`
file, and that removes the sandbox rather than loosening it. The denial of the
installation directories is emitted next to them as depth: how flatpak resolves
a narrow deny against a wide allow is flatpak's rule, and shiro does not claim
to have made `filesystem=home` safe.

**Enforced by `catalog validate` as well**, since 0.3.1. It was not, for one
release, on the reasoning that the validator would have to call into
`src/perms/` and that module survives as an exception only while it stays
sealed. That reasoning weighed the seal correctly and the cost wrongly.

`apply` runs inside the recipe's `post`, on the machine of whoever installed,
and under `rollback = "atomic"` a failure there takes the installation with it:
a mistyped permission is an application that installs and then disappears, and
every gate before that, an image build validating its whole catalog included,
reported success. Deny by default made it worse in the same release, because
profiles stopped being three-word deny lists: the migration that prompted this
was eighty permission strings across eleven applications, none of them looked
at by `validate`.

What resolves the tension is the direction of the call. The validator hands the
declaration to the permission module and prints the sentences it gets back. It
asks; it does not learn. Nothing about sockets, devices or flatpak enters the
engine, and the closed lists stay in the one file that owns them, which is the
part that actually matters: a second copy of the vocabulary inside the engine
would be the copy nobody updates. `CLAUDE.md` now names three callers instead
of two, with that test written down for the next one.

## `perms flatpak apply` emits one invocation, and knows nothing about `:reset`

Applying a profile builds a single `flatpak override` command carrying the
denies and the allows together. It used to issue one per side, and that silently
dropped the allows for any profile whose denies included a resetting one:
`flatpak override` merges into a file it keeps, `filesystem=host:reset` clears
what is in that file, and the second invocation therefore threw away what the
first had just written. Measured on a real install, the override file ended as
`filesystems=!host:reset;!home;!host;` with the granted `xdg-download` gone.

Two fixes were rejected on the way to this one.

**Ordering the arguments** does not apply, because the ordering was never the
variable: inside a single invocation flatpak resolves the whole set and produces
the same result in either order. The split was the bug.

**Special-casing `:reset` in shiro**, by hoisting it to the front or by issuing
resets first, would work and is the wrong shape. `:reset` is flatpak vocabulary,
and teaching the engine that one token means "must come first" is the erosion
the boundary in `CLAUDE.md` warns about, one locally-reasonable exception at a
time. One invocation needs no knowledge of what any token means and fixes the
whole class rather than this token.

The denies are emitted before the allows because that is how a profile reads,
"take everything away, then give this back", and explicitly not because flatpak
requires it.

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

## Distribution: a public release tarball, pinned by the consumer

Same shape as sora: a tarball, with the version and sha256 pinned where it is
consumed. The repository is public, so the kuuhaku-os build fetches the release
with no credential at all.

This replaces the original plan, which kept the repository private and gave the
image build a read token. The token was never the point, it was the cost of
staying closed, and it bought nothing: a build secret to rotate, a failure mode
that only appears in someone else's CI, and an installer whose main path could
not be tested by anyone who did not already have access. Opening it was always
listed as the path that removes the token, and taking it early costs less than
taking it later.

The installer still honours `SHIRO_TOKEN`, which now covers a private fork and
the anonymous rate limit rather than the project itself.

Rejected building from source inside the `Containerfile`: it would drag a Rust
toolchain into the image build for every rebuild, and it would lose the
version-and-digest pinning that makes the sora installation auditable.

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
