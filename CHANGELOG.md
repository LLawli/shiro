# Changelog

What changed, and what a consumer has to do about it. The `--json` payload
carries a `schema` field, and any change a consumer could notice is recorded
here with the version that made it.

## Unreleased

`schema` is **2**. A consumer written against 1 keeps working for everything it
already read: the break is in what is new, listed under each entry.

### Added

- Presentation metadata, which a front end cannot invent and shiro does not
  interpret: `icon` and `keywords` on a menu or an item, and `confirm`,
  `destructive`, `interactive` and `keep_open` on an item. They are emitted in a
  menu listing when set, and omitted otherwise, which for a boolean means it
  appears only when true. ([#2])

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
