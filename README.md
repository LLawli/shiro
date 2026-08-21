# shiro

[Português](README.pt-BR.md)

Curated tooling for immutable Linux, driven by a catalog tree.

> **Status: 0.2.0.** The engine is complete against the contract in
> [docs/architecture.md](docs/architecture.md): the catalog, the executor and
> the permissions module. It ships **no recipes**: the curation lives in a layer
> above it, which for this ecosystem is kuuhaku-os. See
> [catalog/README.md](catalog/README.md).

## The idea

An image-based distro is excellent at being a system and bad at growing one.
Installing an editor, a database or a language toolchain is never hard, but it
is knowledge: which Flatpak id, which container, which Quadlet unit, and which
configuration has to follow. That knowledge evaporates between reinstalls.

shiro turns it into a tree you can navigate and execute. A TOML catalog
declares the tree: menus to walk, items carrying the recipe that installs,
configures, checks and removes something, actions that simply do something, and
lists whose children a command prints while the menu is drawn. The command path
is the tree path:

```sh
shiro install                 # lists the groups under install
shiro install code            # lists the items under install/code
shiro install code vs-code    # runs the recipe for that item
```

The engine knows nothing about Flatpak, podman or distrobox. It resolves a
path, decides privilege, runs the recipe's hooks in order, undoes them when
something fails, and reports as text or JSON. Every mechanism lives in a
recipe, so a new one costs a TOML file rather than a release.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/LLawli/shiro/main/packaging/install.sh | sh
```

Downloads the release tarball, verifies its sha256 against the published
checksum, and installs to `~/.local` with no root, which survives an image
rebase on an atomic distro. `PREFIX=/usr/local` moves it. The artifact is a
static musl binary and needs nothing on the host.

From source, with Rust 1.88 or newer:

```sh
cargo build --release    # target/release/shiro
```

A freshly installed shiro lists three empty menus. That is correct: it is a
catalog engine with no catalog, and it prints the directories a layer can be
placed in.

## Using it

Naming a menu lists it; naming an item runs its recipe. There is no `list` verb,
because a menu with no arguments left to consume *is* the list.

```sh
shiro install code --json               # what a front end reads
shiro install code vs-code              # install, refused if check says it is there
shiro install code vs-code --force      # skip the check gate, nothing else
shiro install code vs-code --dry-run    # print every command, run none of them
shiro install code vs-code --uninstall  # remove it
shiro system lock                       # an action: it runs, nothing is installed
shiro theme pick                        # a list: its children come from a command
shiro theme pick nord                   # runs the list's hook for that entry
```

`--keep-partial` downgrades an atomic rollback to a per-phase one for a single
run, for debugging a recipe without paying the reinstall.

Alongside the tree there are native commands, for the things that require seeing
the engine's own state:

| Command | Purpose |
| --- | --- |
| `shiro doctor` | Which layers loaded, what each contributed, which `mechanism` labels the catalog uses. |
| `shiro catalog validate` | Enforces the schema. Run it before shipping a layer. |
| `shiro catalog sources` | Which layer contributed or overrode each node. |
| `shiro version` | The version, and a digest of the merged catalog. |
| `shiro perms …`, `shiro run …` | Permissions, below. |

Exit codes are contract: `0` fine, `1` a hook or the catalog failed, `2` the
invocation was wrong, `3` shiro declined and nothing happened, `4` a rollback
hook failed and the system is in a state nobody intended.

## Writing a catalog layer

Drop `.toml` files in `/usr/share/shiro/catalog/` (a distro image),
`/etc/shiro/catalog/` (one machine) or `$XDG_DATA_HOME/shiro/catalog/` (one
user). File names do not matter: the path comes from the declarations.

```toml
[[menu]]
path  = "install.code"
title = "Code editors"
order = 20

[[item]]
path        = "install.code.vs-code"
title       = "Visual Studio Code"
mechanism   = "flatpak"        # a label for display, never a dispatch key
privilege   = "user"           # user | system
rollback    = "atomic"         # atomic | phase | none

[item.hooks]
check        = "flatpak info --user com.visualstudio.code"
install      = "flatpak install --user -y flathub com.visualstudio.code"
roll-install = "flatpak uninstall --user -y com.visualstudio.code"
```

Half of a desktop menu is not an installation. A node that has no installed
state is an `[[action]]`, with one hook and nothing to roll back:

```toml
[[action]]
path      = "system.lock"
title     = "Lock the screen"
privilege = "user"

[action.hooks]
run = "loginctl lock-session"
```

A node whose children only exist at run time (the wallpapers on disk, the boxes
that exist, the applications installed) is a `[[list]]`. A command prints the
entries, the catalog declares what runs for the one chosen, and the entry
arrives as `SHIRO_ENTRY`:

```toml
[[list]]
path  = "theme.pick"
title = "Theme"

[list.entries]
command = "kuuhaku-themes --json"   # prints [{"id": "nord", "title": "Nord"}, ...]
ttl     = "5s"                      # how long that answer may be reused

[list.hooks]
run = 'kuuhaku-theme set "$SHIRO_ENTRY"'
```

`shiro theme pick` lists what the generator printed and `shiro theme pick nord`
runs the hook for that entry, refusing an entry the generator does not offer.
The generator prints identities and never commands: what runs is declared by
the catalog, in the layer an administrator controls.

`shiro catalog validate` exits non-zero and names the file, the node and the
problem. The full format is in
[docs/architecture.md](docs/architecture.md), sections 3 and 4.

## Design in one screen

- **The catalog is the command surface.** Subcommands are data, not code. A
  distro or a user can add a whole command group without a shiro release.
- **Four catalog layers**, in increasing precedence: built into the binary,
  `/usr/share/shiro/catalog/`, `/etc/shiro/catalog/`, and the user's own
  under `$XDG_DATA_HOME/shiro/catalog/`. A layer replaces a node wholesale.
- **Four kinds of node:** a `menu` to navigate, an `item` that installs, an
  `action` that simply does something, and a `list` whose children a command
  prints at run time. Neither an action nor a list reports a `status`, because
  nothing about them is installed.
- **Eight hooks per recipe:** `check`, `pre`, `install`, `post`, `roll-pre`,
  `roll-install`, `roll-post`, `uninstall`. Removal is derived from the
  rollback hooks when `uninstall` is absent. An action and a list declare one
  hook, `run`.
- **Rollback is declared, not guessed:** `atomic` undoes everything, `phase`
  undoes only what failed, `none` undoes nothing. A failing rollback is its own
  outcome, with its own exit code.
- **No state.** shiro records nothing about what it installed; `check` asks
  the system, in parallel, under a timeout, never elevated.
- **Rust, for startup.** The binary is ephemeral and spawned once per menu
  navigation step. A listing with checks runs in single-digit milliseconds.

## Permissions

Installing a tool and deciding what it may touch arrive at the same moment, so
shiro owns both. Modestly:

```sh
shiro perms flatpak com.brave.Browser                        # show the overrides
shiro perms flatpak com.brave.Browser deny filesystem=home   # drives flatpak override
shiro perms run brave        # the profile, and the exact bwrap invocation it produces
shiro run brave              # launch it under that profile
```

`shiro perms` always names its backend, so what is being changed is never
inferred. `shiro run` launches native applications under bwrap according to a
declared profile, and an application with no profile runs under a fallback that
grants close to nothing, loudly: every namespace unshared, `/usr` and `/etc`
read only, a tmpfs over the home directory, an emptied environment. It never
runs unconfined. Profiles layer exactly like the catalog does, and the user
layer may loosen one, not only tighten it.

```toml
# ~/.local/share/shiro/profiles/brave.toml
backend = "run"
app     = "brave"

[run]
command    = "/usr/bin/brave-browser"
network    = true
share      = ["wayland", "pipewire"]
devices    = ["dri"]
read-write = ["~/Downloads"]
```

A recipe can declare `[item.permissions]` for what it installs. The engine
records the profile and nothing more: generating the wrapper or `.desktop` that
calls `shiro run` is the recipe's own `post`.

This is not a policy engine and does not try to be firejail. It is the smallest
thing that makes "the browser runs sandboxed even though it is not a Flatpak"
true.

## The menu

shiro renders nothing. The menu is a [Vicinae](https://vicinae.com) extension
that consumes `--json`, draws the children of a node, shows which items are
already present, and invokes install or uninstall explicitly. Other front ends
are welcome and equally external.

The payload carries a `schema` field and is versioned: a breaking change to it
is a breaking change to shiro, and it is recorded in
[CHANGELOG.md](CHANGELOG.md).

## Where it fits

| Project | Role |
| --- | --- |
| [kuuhaku-os](https://github.com/LLawli/kuuhaku-os) | The distro: a Fedora bootc image where the `Containerfile` is the system. |
| [sora](https://github.com/LLawli/sora) | Erases the line between host and container: type a command and it runs, wherever it lives. |
| **shiro** | Turns curated tooling into an executable, navigable tree, and owns what each tool is allowed to touch. |

sora answers "where does this command live". shiro answers "how does this tool
get here, and what is it allowed to touch".

The engine is portable to any host with a POSIX shell. The curation is written
for Fedora bootc and does not pretend otherwise.

## Contributing

Read [CLAUDE.md](CLAUDE.md) first: it holds the rules that are easy to miss and
expensive to miss, starting with the one the whole design rests on, that the
recipe executor never learns a mechanism.
[docs/decisions.md](docs/decisions.md) records what was decided and what was
rejected, so that nothing already investigated gets proposed again from scratch.

## License

MIT.
