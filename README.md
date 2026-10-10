# Everyinit

Everyinit combines two tools for project automation:

- **Evin** runs repository workflows configured with `.flow` manifests.
- **ESH** runs portable automation scripts and can be used on its own.

## Quick start

```sh
git clone https://github.com/dukasn/everyinit.git
cd everyinit
cargo install --path crates/evin
evin init --all
evin run <target>
```

## Documentation

- [Project docs](docs/index.md)
- [Evin book](docs/book/00.index.md)
- [ESH reference](docs/reference/esh/00.index.md)
- [Technical reference](docs/reference/00.index.md)
- [Contributing](docs/contributing/00.index.md)

Everyinit is free software under the [GNU GPLv3](LICENSE).
