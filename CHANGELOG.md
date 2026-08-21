# Changelog

What changed, and what a consumer has to do about it. The `--json` payload
carries a `schema` field, and any change a consumer could notice is recorded
here with the version that made it.

## 0.3.0 (2026-08-21)

### Changed

- **`shiro perms flatpak <app> apply` now denies by default.** It emits every
  class flatpak has, denied by name (`--unshare`, `--nosocket`, `--nodevice`,
  `--disallow`, and `--nofilesystem=host:reset`), and then what the profile
  allows. A profile is now the whole of what an application may do, rather than
  a list of exceptions on top of whatever its manifest currently asks for, so
  an update that starts asking for the session bus does not get it.

  **Every existing flatpak profile has to be reread before this is applied.**
  An application gets nothing it does not name: `socket=wayland` included,
  without which it has no window. What used to be a short deny list is now a
  description of what the application needs.

- What would undo a sandbox can no longer be granted, in a profile or at the
  terminal: `bus=org.freedesktop.Flatpak` (`flatpak-spawn --host`),
  `socket=session-bus` and `socket=system-bus` (the bus unfiltered), and any
  `filesystem=` inside a flatpak installation (`~/.local/share/flatpak`,
  `/var/lib/flatpak`), which is where the override files are. `apply` refuses
  the whole profile rather than applying the rest of it, and there is no flag
  that turns this off. Denying one of them is unaffected: that is the safe
  direction.

- `device=` and the other valued words are now checked against flatpak's own
  lists rather than forwarded. `device=gpu` is refused with the list instead of
  reaching flatpak.

- A `[flatpak]` profile may now declare an empty `allow`, which means what it
  says: nothing beyond the baseline. What is refused instead is a profile with
  no `[flatpak]` section at all.

### Added

- `socket=<name>` and `feature=<name>` in the permission vocabulary, for
  `--socket`/`--nosocket` and `--allow`/`--disallow`. Under deny by default a
  profile has to be able to name the sockets an application needs, and there
  was no word for them.

## 0.2.0 (2026-08-21)

`schema` is **2**. A consumer written against 1 keeps working for everything it
already read: the break is in what is new, listed under each entry.

### Added

- Presentation metadata, which a front end cannot invent and shiro does not
  interpret: `icon` and `keywords` on a menu or an item, and `confirm`,
  `destructive`, `interactive` and `keep_open` on an item. They are emitted in a
  menu listing when set, and omitted otherwise, which for a boolean means it
  appears only when true. ([#2])
- `--yes`, which answers an item's `confirm`. An item that declares one is
  refused (exit 3) until the question is answered: at a terminal shiro asks,
  and everywhere else, `--json` included, `--yes` is the answer. A front end
  that draws its own dialog from the `confirm` text in the listing passes
  `--yes` afterwards. ([#2])

- `[[list]]`, a fourth kind of node: a menu whose children come from a command
  the catalog declares. The generator prints entries (`id`, `title`, and
  optionally `description`, `icon`, `keywords` and a `value`), and the list's
  single `run` hook is what runs for the one chosen, receiving it as
  `SHIRO_ENTRY`. It is for the half of a desktop menu that only exists at run
  time: the wallpapers on disk, the boxes that exist, the applications
  installed. `shiro theme pick` lists, `shiro theme pick nord` runs, and an
  entry the generator does not offer is refused (exit 2) with what it does
  offer. ([#2])
- `ttl` on `[list.entries]`, which keeps what a generator printed under
  `$XDG_CACHE_HOME/shiro/entries/` for that long, so that a front end redrawing
  a level does not respawn the generator on every keystroke. Absent means no
  caching. `SHIRO_LIST_CACHE=0` turns it off for one invocation, and
  `SHIRO_LIST_TIMEOUT` overrides the five seconds a generator gets before it is
  killed. ([#2])
- `SHIRO_ENTRY` in a hook's environment, set for the entry chosen out of a list
  and absent everywhere else. ([#2])

- `[[action]]`, a third kind of node: one `run` hook, no `check`, no rollback,
  no removal. It is for what a desktop menu is mostly made of, which is things
  that happen rather than things that become present. In a listing it carries
  `"kind": "action"` and **no `status` field at all**, rather than the
  `"unknown"` an item without a `check` reports. `--force`, `--uninstall` and
  `--keep-partial` are refused on one. Running it emits the phase `run` and the
  outcome `ok`. ([#2])

### Changed

- `kind` in a payload can now be `"action"` or `"list"`, and neither carries a
  `status`. A consumer that switches on `kind`, or that reads `status`
  unconditionally, has to handle both. `kind` at the top of a listing is now
  `"menu"` or `"list"` for the same reason: both are navigated into the same
  way, and a consumer that treats `menu` as somewhere to go treats `list` the
  same. This is the break that makes `schema` 2. ([#2])

- An item declaring `confirm` no longer runs unattended without `--yes`. No
  catalog declared the field before this release, so nothing that exists today
  changes behaviour. ([#2])

- `SHIRO_SUDO_PROBE=0` drops the `sudo true` that runs before the first hook of
  a `system` item. It is for an elevator that keeps no credential cache, where
  the probe is a password dialog that fills nothing: a front end routing through
  `pkexec` to the desktop's polkit agent then pays one dialog for a one-hook
  recipe rather than two. What it gives up is the clean refusal, since a
  cancelled password then arrives as the first hook failing. No payload changed,
  so `schema` stays 1. ([#2])

[#2]: https://github.com/LLawli/shiro/issues/2

## 0.1.1 (2026-08-17)

### Fixed

- `shiro perms flatpak <app> apply` issued one `flatpak override` invocation per
  side, and a deny that resets (`filesystem=host:reset`) discarded the allows the
  first invocation had just written. It now emits a single invocation carrying
  both. No payload or command surface changed, so `schema` stays 1. ([#1])

[#1]: https://github.com/LLawli/shiro/issues/1

## 0.1.0 (2026-08-13)

The first release: the engine is complete against `docs/architecture.md`, and
the curation that fills it is not here (see `catalog/README.md`).

### The catalog is the command surface

- Four layers load and merge per node path: built-in (embedded), image
  (`/usr/share/shiro/catalog`), machine (`/etc/shiro/catalog`), user
  (`$XDG_DATA_HOME/shiro/catalog`). `SHIRO_ROOT` reroots the two system layers.
- An argument path resolves against the merged tree. A menu lists its children,
  an item runs its recipe.
- `check` runs in parallel for a menu's children, bounded by a three second
  timeout (`SHIRO_CHECK_TIMEOUT`), never elevated.
- `shiro doctor`, `shiro version`, `shiro catalog validate`, `shiro catalog
  sources`.

### Recipes run, and are undone when they fail

- `check` gates, then `pre`, `install`, `post`, with the item's rollback policy
  deciding what a failure undoes.
- `--force`, `--dry-run`, `--keep-partial`, `--uninstall`.
- Elevation is `sudo` (`SHIRO_SUDO` replaces it), prompting once before anything
  runs.
- Exit codes: 0 fine, 1 a hook or the catalog failed, 2 the invocation was
  wrong, 3 shiro declined and nothing happened, 4 a rollback hook failed.

### Permissions

- `shiro run <app>` launches a native application under bwrap, from a declared
  profile or from a fallback that grants close to nothing and says so.
- `shiro perms run <app>` prints the profile and the exact invocation;
  `shiro perms flatpak <app> [show|allow|deny|reset|apply]` drives
  `flatpak override`.
- An item's `[item.permissions]` is recorded into the profile registry between
  `install` and `post`, and taken back out if the transaction is undone.

### Structured output

`schema` is 1. Payload kinds: `menu`, `doctor`, `version`, `validation`,
`sources`, `profile`, and the `phase` and `result` objects a running recipe
emits, one per line.
