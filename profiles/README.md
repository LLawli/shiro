# The built-in profile layer

Every `.toml` here is embedded in the binary at build time and resolved as the
lowest-precedence layer, exactly like the catalog. One file per application,
named after it: `brave.toml` is the profile for `shiro run brave`.

It is empty for the same reason `catalog/` holds no recipes. A profile says what
one application on one kind of desktop may reach, and that is curation: it
belongs in the image layer (`/usr/share/shiro/profiles/`), on the machine
(`/etc/shiro/profiles/`), or in a home directory
(`$XDG_DATA_HOME/shiro/profiles/`).

The format is in [../docs/architecture.md](../docs/architecture.md), section 9.
`shiro perms run <app>` prints the profile in effect and the exact bwrap
invocation it produces, which is the way to check one before trusting it.
