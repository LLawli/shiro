# shiro

[Português](README.pt-BR.md)

Curated tooling for immutable Linux, driven by a catalog tree.

> **Status: planning.** The design is settled and written down; the engine is
> not implemented yet. Read [docs/vision.md](docs/vision.md) for the idea,
> [docs/architecture.md](docs/architecture.md) for the contract, and
> [docs/decisions.md](docs/decisions.md) for why it is shaped that way.

## The idea

An image-based distro is excellent at being a system and bad at growing one.
Installing an editor, a database or a language toolchain is never hard, but it
is knowledge: which Flatpak id, which container, which Quadlet unit, and which
configuration has to follow. That knowledge evaporates between reinstalls.

shiro turns it into a tree you can navigate and execute. A TOML catalog
declares menus and items; each item carries the recipe that installs,
configures, checks and removes it. The command path is the tree path:

```sh
shiro install                 # lists the groups under install
shiro install code            # lists the items under install/code
shiro install code vs-code    # runs the recipe for that item
```

The engine knows nothing about Flatpak, podman or distrobox. It resolves a
path, decides privilege, runs the recipe's hooks in order, undoes them when
something fails, and reports as text or JSON. Every mechanism lives in a
recipe, so a new one costs a TOML file rather than a release.

## Design in one screen

- **The catalog is the command surface.** Subcommands are data, not code. A
  distro or a user can add a whole command group without a shiro release.
- **Four catalog layers**, in increasing precedence: built into the binary,
  `/usr/share/shiro/catalog/`, `/etc/shiro/catalog/`, and the user's own
  under `$XDG_DATA_HOME/shiro/catalog/`.
- **Eight hooks per recipe:** `check`, `pre`, `install`, `post`, `roll-pre`,
  `roll-install`, `roll-post`, `uninstall`. Removal is derived from the
  rollback hooks when `uninstall` is absent.
- **No state.** shiro records nothing about what it installed; `check` asks
  the system, in parallel and under a timeout.
- **Rust, for startup.** The binary is ephemeral and spawned once per menu
  navigation step.

## Permissions

Installing a tool and deciding what it may touch arrive at the same moment, so
shiro owns both. Modestly:

```sh
shiro perms flatpak brave --nofilesystem=home   # drives flatpak override
shiro perms run brave                           # the bwrap profile for a native app
shiro run brave                                 # launches it under that profile
```

`shiro perms` always names its backend, so what is being changed is never
inferred. `shiro run` launches native applications under bwrap according to a
declared profile, and an application with no profile runs under a fallback that
grants close to nothing, loudly. Profiles layer exactly like the catalog does.

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

## License

MIT.
