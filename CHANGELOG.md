# Changelog

## 0.3.0

- **Format 3.0.0.** The `::: exercise` block is gone. A deck writes `::: flashcard`, and a checkpoint writes `::: question`: the names the learner sees. The checker refuses format 2, and it names the new block at each `::: exercise`. A block with no id is still `e1`, `e2`, so no id moves. To migrate, change the blocks and write `format: 3.0.0` at the root.
- The label `exercise` is now `flashcard`, and `question` is new.
- A flashcard between two cards is an inline flashcard. A flashcard after the last card shows in the sets only, as before.
- The bundle goes to version 7. A deck holds `flashcards`, and each one carries `before`, the index of the card it stands before, when it is inline. A checkpoint holds `questions`, of their own type.

## 0.2.1

- A legend's meaning renders its `$…$` math as inline MathML, as a card does. The text around it stays escaped. A `$` with no partner is a fault at its line.

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
