# xeon

**xeon** is the [.xeo](https://github.com/arozoid/xeo) package manager!

xeon provides a small command-line interface for creating, installing, removing, searching, building, and upgrading packages while keeping the package format close to the layout used by the xeon runtime itself.

packages are built around three directories — `lib/`, `bin/`, and `pkg/` — which lets a package contain `.xeo` libraries, extension binaries, and package metadata without needing a complicated package format.

## getting started

initialize the local xeon tree with:

```sh
xeon init
```

this creates the `~/.xeon` install tree and its endpoints file, after which packages can be installed from local paths, archives, git repositories, endpoints, or simply by name.

if the `.xeo` interpreter isn't installed yet, `bootstrap` can place it directly into `~/.xeon/bin/xeo`:

```sh
xeon bootstrap
```

## packages

a package is just a tree following the xeon layout:

```text
package/
├── lib/
├── bin/
└── pkg/
```

`lib/` contains `.xeo` libraries, `bin/` contains executable extensions, and `pkg/` contains the metadata xeon needs to identify and manage the package.

because the package layout follows the same structure used by the installed xeon environment, installing a package doesn't require translating its contents into some completely different filesystem structure.

### package metadata

each package has a TOML manifest at `pkg/<name>.toml`. `name`, `version`, and `description` identify it; `depends` lists required package names; and `lib` and `bin` list the files it owns under the matching directories:

```toml
name = "printc"
version = "0.1.0"
description = "ANSI color printing for .xeo"
depends = []
lib = ["printc.xeo"]
bin = ["printc"]
```

`origin` is optional metadata xeon records when a package is installed from a path, git source, or endpoint.

## endpoints

xeon doesn't depend on one central package repository, instead allowing package endpoints to be configured locally and searched as a collection.

an endpoint is simply another tree using the same `lib/`, `bin/`, and `pkg/` layout, meaning a package repository can be a directory, a git repository, or another source that provides the expected structure.

packages can be referenced through a specific endpoint:

```text
endpoint/pkg
```

or searched across every configured endpoint with a normal package name:

```sh
xeon install printc
```

this keeps package discovery separate from package installation, so adding another endpoint expands where xeon can find packages without changing the package format itself.

## installing packages

packages can come from several different sources, with xeon accepting a local package tree, an archive, a git url, a named endpoint package, or a bare package name searched across configured endpoints.

```sh
xeon install ./my-package
xeon install ./my-package.tar.gz
xeon install https://example.com/my-package.git
xeon install community/printc
xeon install printc
```

`add` can be used as an alias for `install` when that reads better:

```sh
xeon add printc
```

installed packages can be removed with:

```sh
xeon remove printc
```

or its shorter alias:

```sh
xeon rm printc
```

## managing packages

installed packages can be listed with:

```sh
xeon list
```

while `info` shows the metadata for an installed package or a package available through an endpoint:

```sh
xeon info printc
```

package discovery searches every configured endpoint for matching names or queries:

```sh
xeon search color
```

git endpoints can be refreshed from their origins with:

```sh
xeon update
```

and installed packages can be upgraded from their recorded origins with:

```sh
xeon upgrade
```

## creating packages

new packages can be scaffolded with:

```sh
xeon new my-package
```

which creates a package tree that can immediately be developed, installed locally, or shared with another xeon installation.

once the package is ready, `build` turns the package tree into a compressed archive containing its `lib/`, `bin/`, and `pkg/` directories:

```sh
xeon build my-package
```

producing an archive in the form:

```text
my-package-<version>.tar.gz
```

the resulting archive can then be installed directly with `xeon install`.

## endpoints

endpoints are managed through the `endpoint` command:

```sh
xeon endpoint
```

since endpoints are just package trees following the same layout as packages and the local install tree, a repository doesn't need to follow a completely separate convention just to work with xeon.

## maintenance

xeon also includes a few commands for keeping its local environment tidy and checking for common problems.

```sh
xeon doctor
```

runs basic diagnostics for the current machine, while:

```sh
xeon clean
```

empties the download cache at `~/.xeon/cache/dl`.

the installed xeon version can be checked with:

```sh
xeon version
```

## commands

```text
init       scaffold the ~/.xeon install tree and endpoints file
install    install a package from a path, archive, git url, endpoint, or name
remove     remove an installed package and its files
list       list installed packages
search     search every endpoint for packages matching a query
info       show metadata for an installed package or endpoint package
update     refresh git endpoints from their origin
upgrade    upgrade installed packages from their recorded origins
endpoint   manage package endpoints
new        scaffold a new package tree
build      package a package tree into a <name>-<version>.tar.gz
bootstrap  install the xeo interpreter binary into ~/.xeon/bin/xeo
doctor     run basic diagnostics
clean      empty the download cache
version    print the xeon version
```

most commands also have short aliases where they make sense, such as `add` for `install`, `rm` for `remove`, and `ls` for `list`.

## the idea

xeon is intentionally built around a very small package model, with packages, endpoints, and the installed environment all sharing the same basic layout so that the package manager doesn't need to maintain a complicated abstraction between where a package comes from and where its files eventually end up.

the `.xeo` ecosystem can therefore grow through ordinary directories and git repositories as well as packaged archives, while the package manager handles the bookkeeping, discovery, caching, and upgrades around them.
