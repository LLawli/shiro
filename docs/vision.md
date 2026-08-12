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

## What jibril is

jibril is the engine that turns curated knowledge into something executable and
navigable. A TOML catalog declares a tree of menus and items; each item carries
the recipe that installs, configures, checks and removes it. jibril parses the
tree, resolves a command path against it and runs the recipe.

The command path *is* the tree path:

```sh
jibril install                 # lists the groups under install
jibril install code            # lists the items under install/code
jibril install code vs-code    # runs the recipe for that item
```

The engine knows nothing about Flatpak, podman or distrobox. It knows how to
find a node, decide privilege, run hooks in order, undo them when something
fails, and report the result as text or as JSON. Every mechanism lives in a
recipe, which is why a new one costs a TOML file and not a release.

## What jibril is not

- **Not a package manager.** It does not resolve dependencies, does not own a
  package database, does not know versions. It drives the tools that do.
- **Not a menu.** It renders no interface. The menu is a Vicinae extension that
  consumes structured output. Other front ends are equally welcome, and equally
  external.
- **Not a permission system.** Recipes may set permissions today, at the
  discretion of whoever writes them. That responsibility belongs to
  [shiro](#the-ecosystem) once it exists.
- **Not tied to one distro.** The curation is written for Fedora bootc, so a
  hypothetical `rpm-ostree` layering module would only work there. The engine
  itself assumes nothing beyond a POSIX shell.

## The ecosystem

jibril is one of four projects that only make full sense together, and each of
which has to stand alone.

| Project | Role |
| --- | --- |
| `kuuhaku-os` | The distro: a Fedora bootc image where the `Containerfile` *is* the system. |
| `sora` | Erases the line between host and container: type a command and it runs, wherever it lives. |
| `jibril` | Turns curated tooling into an executable, navigable tree. |
| `shiro` | Permissions and sandboxing across distrobox boxes, Flatpaks and native apps run under bwrap. |

The boundaries matter more than the overlap. sora answers "where does this
command live"; jibril answers "how does this tool get here"; shiro answers
"what is this tool allowed to touch". A feature that answers two of those
questions at once is in the wrong repository.

The concrete consequence today: a jibril recipe may create a distrobox box, and
that box becomes visible to sora through sora's own indexing, with no coupling
between the two. When shiro lands, it reads what exists on the system and
applies policy; jibril does not have to grow a permission model to meet it.

## Who consumes jibril

Two consumers, and the design answers to both.

**A human at a terminal.** Types `jibril install code`, reads a list, types the
item. Wants clear errors and no surprises.

**A menu, currently a Vicinae extension.** Opens a node, draws the children,
shows which items are already present, and invokes install or uninstall
explicitly based on that. This consumer runs jibril once per navigation step,
which is the single hardest constraint in the project: a menu that stutters is
a menu nobody opens. It is the reason the engine is a compiled binary that does
no eager work, and the reason `check` has a batch mode.

## The name

Jibril is the Flügel from *No Game No Life*: an obsessive collector of
knowledge, ancient and slightly unhinged about acquiring more. A tool whose
entire purpose is accumulated curation could not be called anything else. The
distro is Kuuhaku (『　』, the team name), and the siblings that give it their
names are already taken by the other two projects.
