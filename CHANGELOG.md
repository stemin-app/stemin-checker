# Changelog

## 0.2.0

- The source is public, under MIT OR Apache-2.0. The workspace has three crates: `stemin-format`, `stemin-compile` and `stemin`.
- `stemin check` runs the same compile as the import in the Stemin app. Every fault has a file and a line.
- **Security:** the compiler re-emits the MathML of the LaTeX renderer through an allowlist. Markup inside `\text{…}` or `\operatorname{…}` does not reach the page. Version 0.1.0 let it through: upgrade.
- An SVG figure stops at the end of its root. Its `class` and its root `width` and `height` do not survive, and each `id` takes a prefix from a hash of the file.
- A plain link stays a link only to `https:` or `mailto:`, with `rel="noopener noreferrer"`.
- New limits: a `.md` file is 512 KB at most, a path is 16 levels deep at most, blocks nest one level, a domain compiles to 32 MB of HTML at most, and a plot has caps on its numbers and its counts (CONTENT-MODEL.md §10.1).
- `stemin check` refuses a symbolic link and never follows one, and it reads no file past its cap.

## 0.1.0

- The first release: binaries for Linux, macOS and Windows.
