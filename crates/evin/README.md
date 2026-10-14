# Evin

Evin runs repository workflows defined by `.flow` manifests in `.evin/`. Use
it to check and run tasks, synchronize Git hooks, and automate project releases.

Install from the Everyinit checkout:

```sh
cargo install --path crates/evin
```

Initialize and run a workflow:

```sh
evin init --all
evin check
evin run <target>
```

## Documentation

- [Evin book](../../docs/book/00.index.md)
- [CLI reference](../../docs/reference/10.cli-reference.1000.md)
- [Workspace configuration](../../docs/reference/20.workspace-configuration.4500.md)
- [Built-in actions](../../docs/reference/actions/00.index.md)

Licensed under [GNU GPL version 3 only](../../LICENSE) (`GPL-3.0-only`).
