# baseballyama/oxc — the oxc fork for rsvelte

This is a fork of [oxc-project/oxc](https://github.com/oxc-project/oxc), kept for
[rsvelte](https://github.com/baseballyama/rsvelte). It is not an official oxc
repository, and nothing here is released by the oxc team.

> [!NOTE]
> GitHub shows this file (`.github/README.md`) in place of oxc's own
> [`README.md`](../README.md), which this fork leaves untouched. That way a rebase
> on upstream never conflicts on the README.

## Branches

| Branch | What it is |
| --- | --- |
| `rsvelte-pin` | The upstream release rsvelte builds on, with rsvelte's patches on top. rsvelte pins its oxc crates to a commit of this branch. |
| `main` | A mirror of upstream `main`. It carries no patches. |

## The patches

[**#1 lists every patch on `rsvelte-pin`**](https://github.com/baseballyama/oxc/issues/1),
grouped by area (parser, semantic, codegen, formatter, oxfmt, ...), with what
each one is for and whether it has gone upstream.

Each commit on `rsvelte-pin` is one change that could be sent to oxc as its own
pull request. The goal is to upstream them, until rsvelte no longer needs a fork.

## Updating the fork

`rsvelte-pin` is rebased on the commit of the latest oxc release (for example the
`oxlint_v1.95.0` tag), not on upstream's moving `main`:

```sh
git fetch upstream --tags
git rebase --onto <release commit> <previous base> rsvelte-pin
# regenerate the snapshots the patches touch
cargo coverage -- parser
(cd apps/oxfmt && pnpm run build-test && pnpm run conformance:prettier)
git push --force-with-lease origin rsvelte-pin
```

Then:

1. Update [#1](https://github.com/baseballyama/oxc/issues/1): the new base, the new
   commit of each patch, and any patch dropped because upstream now has it.
2. Point rsvelte's `Cargo.toml` at the new head of `rsvelte-pin`.

This README is the first commit on `rsvelte-pin`, and only adds a file upstream does
not have, so it never conflicts in a rebase.
