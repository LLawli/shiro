# Vision and scope

## The problem

An immutable, image-based distro is excellent at *being* a system and bad at
*growing* one. The image is the distro: reproducible, atomic, rollback-capable.
But the moment a user wants an editor, a database, a game launcher or a
language toolchain, that guarantee stops helping. The honest answers available
today are all mechanisms, not answers:

- `flatpak install` needs the reverse-DNS app id, which nobody remembers;
- a development toolchain belongs in a container, which means knowing distrobox
  and then knowing which base image;
- a service like Postgres belongs in a plain container that survives reboot,
  which means writing a Quadlet unit by hand;
- and half of these are useless right after installation, because the part that
  matters is the configuration that follows.

None of that is hard. All of it is *knowledge*, and it evaporates between one
reinstall and the next. The curation is the product; the commands are trivia.

## What shiro is

shiro is the engine that turns curated knowledge into something executable and
navigable. A TOML catalog declares a tree of menus and items; each item carries
the recipe that installs, configures, checks and removes it. shiro parses the
tree, resolves a command path against it and runs the recipe.

The command path *is* the tree path:

```sh
shiro install                 # lists the groups under install
shiro install code            # lists the items under install/code
shiro install code vs-code    # runs the recipe for that item
```

The engine knows nothing about Flatpak, podman or distrobox. It knows how to
find a node, decide privilege, run hooks in order, undo them when something
fails, and report the result as text or as JSON. Every mechanism lives in a
recipe, which is why a new one costs a TOML file and not a release.

## What shiro is not

- **Not a package manager.** It does not resolve dependencies, does not own a
  package database, does not know versions. It drives the tools that do.
- **Not a menu.** It renders no interface. The menu is a Vicinae extension that
  consumes structured output. Other front ends are equally welcome, and equally
  external.
- **Not a general sandboxing framework.** shiro owns permissions, but modestly:
  Flatpak overrides and a bwrap wrapper for native applications, driven by
  declared profiles. It does not aspire to be a policy engine.
- **Not tied to one distro.** The curation is written for Fedora bootc, so a
  hypothetical `rpm-ostree` layering module would only work there. The engine
  itself assumes nothing beyond a POSIX shell.

## The ecosystem

shiro is one of three projects that only make full sense together, and each of
which has to stand alone.

| Project | Role |
| --- | --- |
| `kuuhaku-os` | The distro: a Fedora bootc image where the `Containerfile` *is* the system. |
| `sora` | Erases the line between host and container: type a command and it runs, wherever it lives. |
| `shiro` | Turns curated tooling into an executable, navigable tree, and owns what each tool is allowed to touch. |

The boundary that matters is with sora. sora answers "where does this command
live"; shiro answers "how does this tool get here, and what is it allowed to
touch". A feature that answers the first question belongs in sora.

The concrete consequence: a shiro recipe may create a distrobox box, and that
box becomes visible to sora through sora's own indexing, with no coupling
between the two.

Permissions used to be a fourth project, also named shiro, scoped as a general
sandboxing layer for boxes, Flatpaks and native applications. That project was
cancelled and its name taken by this one. What survives is a deliberately
smaller module, described in the next section: the ambition was the part that
made it never start.

## The permissions module

Installing a tool and deciding what it may touch are different questions, but
they arrive at the same moment: the point where something new lands on the
system is exactly when its blast radius is decided, and the only time anyone
thinks about it. Splitting that across two projects meant one of them would
never be written, which is what happened.

So shiro keeps both, and keeps the second one small. Two surfaces:

**`shiro perms`**, for applications that already have a permission mechanism of
their own. In practice that means Flatpak: shiro drives `flatpak override`
rather than inventing anything. The backend is always explicit in the command,
because a permission tool that hides which mechanism it is talking to is a
permission tool nobody can audit.

**`shiro run`**, for native applications, which have no such mechanism. It
launches a program under bwrap according to a declared profile. An application
with no profile still runs, under a fallback that grants close to nothing: the
failure mode is an application that cannot reach what it was not given, which
is loud, rather than an application silently running unconfined.

The ambition stops there. This is not a policy engine, has no model of trust,
and does not try to be firejail. It is the smallest thing that makes "the
browser runs sandboxed even though it is not a Flatpak" true.

## Who consumes shiro

Two consumers, and the design answers to both.

**A human at a terminal.** Types `shiro install code`, reads a list, types the
item. Wants clear errors and no surprises.

**A menu, currently a Vicinae extension.** Opens a node, draws the children,
shows which items are already present, and invokes install or uninstall
explicitly based on that. This consumer runs shiro once per navigation step,
which is the single hardest constraint in the project: a menu that stutters is
a menu nobody opens. It is the reason the engine is a compiled binary that does
no eager work, and the reason `check` has a batch mode.

## The name

Shiro is the younger half of 『　』 in *No Game No Life*: the calculating one,
the one who holds the whole board in memory and does not miss. A tool that is
one part accumulated curation and one part deciding what a program may touch
fits her better than it fits anyone else in that story.

The distro is Kuuhaku (『　』, the team name) and the other tool is sora, so the
two siblings that make up the team now name the two tools that make up the
system. That symmetry was not the plan, but it is better than the plan was.

This project was called jibril until it absorbed the permissions work. The old
name is retired, not reserved.
