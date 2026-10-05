# stemin-checker

The checker and the format of [Stemin](https://stemin.app) courses.

A Stemin course is a git repository of Markdown files. The `stemin` checker reads such a repository on your computer and runs the same checks as the import in the Stemin app: the same parser and the same compiler. A repository that passes the checker imports. A repository that fails gets a list of faults, each with its file and its line.

## Download

Each [release](https://github.com/stemin-app/stemin-checker/releases) has one binary for each platform, and a `SHA256SUMS` file.

| File | For |
|---|---|
| `stemin-x86_64-linux` | Linux on Intel or AMD |
| `stemin-aarch64-linux` | Linux on ARM |
| `stemin-x86_64-macos` | macOS on Intel |
| `stemin-aarch64-macos` | macOS on Apple silicon |
| `stemin-x86_64-windows.exe` | Windows |

```sh
curl -LO https://github.com/stemin-app/stemin-checker/releases/latest/download/stemin-x86_64-linux
curl -LO https://github.com/stemin-app/stemin-checker/releases/latest/download/SHA256SUMS
grep stemin-x86_64-linux SHA256SUMS | sha256sum -c
chmod +x stemin-x86_64-linux
```

On macOS, use `shasum -a 256` in place of `sha256sum`.

## Build from source

You need Rust. The repository pins its version in `rust-toolchain.toml`, and [rustup](https://rustup.rs) installs it.

```sh
cargo install --locked --git https://github.com/stemin-app/stemin-checker stemin
```

## Use it

```sh
stemin check path/to/your/course
```

The command lists every fault and exits with 1, or says that the repository is valid and exits with 0. The guide, with a CI example, is at [stemin.app/docs/check](https://stemin.app/docs/check).

## The crates

| Crate | What it holds |
|---|---|
| `stemin-format` | The format: the tree, the blocks, the front matter, the ids, and the structural checks. It has no renderer. |
| `stemin-compile` | The content model, and the compiler: Markdown to HTML, LaTeX to MathML, the SVG allowlist, the plots. |
| `stemin` | The command line. |

`stemin-format` and `stemin-compile` build for `wasm32`, because the Stemin app runs them in the browser. [CONTENT-MODEL.md](CONTENT-MODEL.md) is the specification of the format.

## Development

```sh
just check    # format, lint (native and wasm), and test
just tools    # build the five release binaries into target/tools
```

The lints are strict: no `unwrap`, `expect` or `panic` outside tests, no `as` casts, and `#[expect(reason = "...")]` in place of `#[allow]`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.

Unless you state otherwise, any contribution that you intentionally submit for inclusion in this work, as defined in the Apache-2.0 license, is dual licensed as above, with no additional terms or conditions.
