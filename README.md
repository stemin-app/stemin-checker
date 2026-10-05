# stemin-checker

The checker for [Stemin](https://stemin.app) courses. It reads a course repository on your computer and runs the same checks as the import in the app. A repository that passes the checker imports. A repository that fails gets a list of faults, each with its file and its line.

This repository holds the releases. Each release has one binary for each platform, and a `SHA256SUMS` file.

| File | For |
|---|---|
| `stemin-x86_64-linux` | Linux on Intel or AMD |
| `stemin-aarch64-linux` | Linux on ARM |
| `stemin-x86_64-macos` | macOS on Intel |
| `stemin-aarch64-macos` | macOS on Apple silicon |
| `stemin-x86_64-windows.exe` | Windows |

## Get it

```sh
curl -LO https://github.com/stemin-app/stemin-checker/releases/latest/download/stemin-x86_64-linux
curl -LO https://github.com/stemin-app/stemin-checker/releases/latest/download/SHA256SUMS
grep stemin-x86_64-linux SHA256SUMS | sha256sum -c
chmod +x stemin-x86_64-linux
```

On macOS, use `shasum -a 256` in place of `sha256sum`.

## Use it

```sh
./stemin-x86_64-linux check path/to/your/course
```

The full guide, with a CI example, is at [stemin.app/docs/check](https://stemin.app/docs/check).
