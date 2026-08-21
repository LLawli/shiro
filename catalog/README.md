# The built-in catalog layer

Every `.toml` here is embedded in the binary at build time and loaded as the
lowest-precedence layer. It holds the root nodes and stops there.

**Recipes do not belong here.** They belong in a layer above: the image layer
for a distro (`/usr/share/shiro/catalog/`), `/etc/shiro/catalog/` for one
machine, `$XDG_DATA_HOME/shiro/catalog/` for one user. The reasoning is in
[../docs/decisions.md](../docs/decisions.md), under "The curation does not ship
inside the engine": a mechanism changes on someone else's schedule, and a recipe
that lives here turns every such change into a shiro release.

A layer above adds children by declaring them under one of these paths:

```toml
# /usr/share/shiro/catalog/code.toml

[[menu]]
path  = "install.code"
title = "Code editors"
order = 20

[[item]]
path        = "install.code.vs-code"
title       = "Visual Studio Code"
mechanism   = "flatpak"
privilege   = "user"

[item.hooks]
check        = "flatpak info --user com.visualstudio.code"
install      = "flatpak install --user -y flathub com.visualstudio.code"
roll-install = "flatpak uninstall --user -y com.visualstudio.code"
```

A node that does something instead of installing something is an `[[action]]`,
with one hook:

```toml
[[action]]
path      = "system.lock"
title     = "Lock the screen"
privilege = "user"

[action.hooks]
run = "loginctl lock-session"
```

The format is specified in [../docs/architecture.md](../docs/architecture.md),
sections 3 and 4. `shiro catalog validate` enforces it, and it is the thing to
run before shipping a layer.

One rule applies here and nowhere else: **a hook in this layer cannot be a
`{ file = "..." }` script**, because this layer is inside the binary and has no
directory on disk. Inline it.
