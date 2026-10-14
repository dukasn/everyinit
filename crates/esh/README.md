# ESH — Evin Shell

ESH is Everyinit's shell language runtime for project automation. Use it on
its own or to run commands in an [Evin workflow](../../README). Evin is
currently supported for local development on Linux; see the
[platform policy](../../CI.md).

```esh
#!/usr/bin/env esh
set -e

echo "Building project"
cargo build
```

Run a script or a command:

```sh
esh script.esh
esh -c 'echo hello'
```

## Documentation

- [ESH overview](../../docs/reference/esh/00.index.md)
- [Writing scripts](../../docs/reference/esh/10.writing-scripts.md)
- [Language reference](../../docs/reference/esh/90.language-reference.3000.md)
- [Runtime reference](../../docs/reference/esh/100.runtime-reference.3010.md)

Licensed under [GNU GPLv3](../../LICENSE).
