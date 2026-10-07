# Stemin — Content Model

Stemin is **Duolingo for mathematics and physics**. The structure is borrowed 1:1, because it
is proven, and renamed to fit the subject:

| Duolingo | Stemin | Is |
|---|---|---|
| Course (a language) | **Domain** | a build unit, one bundle (math, physics) |
| Section | **Section** | a group of topics (algebra, calculus) |
| Unit / level | **Topic** | a chapter in the menu |
| Lesson | **Deck** | one concept, built across several cards |
| — | **Card** | one step of a deck: one idea, read in the scroll |
| Exercise | **Exercise** | one practice item, reviewed as a flashcard, FSRS-scheduled |
| Guidebook | **Reference** | a result the learner looks up, with its derivation |
| Unit test | **Checkpoint** | an exam: its own questions, in a folder (the app says Exam) |

**The deck is the unit, and the only thing an author teaches with.** A deck holds its cards,
its exercises and its references. Everything above a deck is collected from the decks under it:
a chapter's flashcards and references are its decks', a section's are its chapters', and the
domain's are its sections'. Nothing at those levels is declared. A checkpoint is the one other
thing an author writes, and it stands apart: its questions are its own.

Two things Duolingo does not have, kept **strictly apart**:
- a **derivation** says why a **reference** holds. It lives with that reference.
- a **solution** is the worked answer to an **exercise**. It is reached from that exercise.

A reference's derivation and an exercise's solution are different entities with different
homes. They are never mixed.

**The filesystem is the structure.** Domain, section, and topic are directories, each with an
`index.md`; a deck is a `.md` file. The build renders everything to real HTML (prose, native
MathML, themed SVG islands) and emits **one bundle per domain**. The app loads every domain's
bundle and merges them into one graph. Spaced-repetition state is not content; it lives per
user on the device.

---

## 1. The tree

Content lives in a **content repository**, which the learner adds to the app by its URL. One
repository holds one or more domains. The app ships no content of its own.

### What carries what

| Carries | What |
|---|---|
| the **directory** | containment |
| the **filename stem** | the id |
| the **depth** | the kind |
| **`order:`** in the container's `index.md` | the order |
| **front matter** | the title, and the node's own metadata |
| **`:::` blocks** and one fenced language, `plot` | all content |

Three reserved names, and nothing else: **`index.md`**, **`checkpoints/`**, **`assets/`**.

### The shape

```
index.md                         the repository: format, author, description, order
phy/                             domain     id: phy
├── index.md                       title, lang, labels, order
├── eam/                         section     id: eam
│   ├── index.md                   title, tiers, order
│   ├── fields/                  topic      id: fields
│   │   ├── index.md               title, tier, requires, order
│   │   ├── field.md             deck       id: field   (cards, exercises, references)
│   │   ├── electric-field.md    deck       id: electric-field
│   │   └── assets/                inlined at build (§1.4)
│   │       └── flux.png
│   └── direct-current/
│       ├── index.md
│       ├── charge.md
│       └── ohms-law.md
└── checkpoints/                 the domain's checkpoints, beside its sections
    ├── index.md                   order
    └── mit/                     a folder, nested as deep as the limit below allows
        ├── index.md               title, order
        └── 2020.md              checkpoint  id: 2020
```

Depth decides the kind, so no file declares one:

| Path | Kind |
|---|---|
| `index.md` | the repository |
| `<domain>/index.md` | domain |
| `<domain>/<section>/index.md` | section |
| `<domain>/<section>/<topic>/index.md` | topic |
| `<domain>/<section>/<topic>/<deck>.md` | deck |
| `<domain>/checkpoints/index.md`, and `index.md` in any folder under it | checkpoint folder |
| any other `.md` file under `<domain>/checkpoints/` | checkpoint |

`checkpoints/` is not a section, so a domain's `order` does not list it.

An `index.md` holds front matter and nothing else. The check refuses text after it.

Two limits hold for every file. A `.md` file is **512 KB** at most: a deck that long is many
decks. A path is **16 levels** at most below the repository root, its own name included, so
`phy/checkpoints/mit/2020/fall/q.md` is six. The check reads nothing deeper.

On disk, the check reads files and directories only. It never follows a **symbolic link**, and
it refuses one, and it refuses a device, a pipe or a socket: a forge serves none of them as
content.

### 1.1 `order:`, and the bijection

A container's `index.md` lists its children by id, in the order the learner meets them. The
filesystem cannot carry this: the electromagnetism section runs `fields → direct-current →
components → dc-analysis → electromagnetic → alternating-current`, and alphabetical order would
open the course on its last chapter.

`order` is **validated as a bijection with the directory, both ways**:

- a file present but **not listed** is an error;
- a name listed but **not present** is an error.

So the list can never drift from the tree. `order` is **optional**; absent, the children sort
alphabetically. It is a **sort key, not a declaration of existence** — the directory declares
what exists.

> **The known cost.** Adding a deck is two edits: the file, and its parent's `order`. This was
> weighed in 2026-09-22 against numbered filenames (`10-field.md`) and against collapsing a
> topic into one file, and settled in favour of clean filenames and granular files. The
> bijection check is what makes the second edit impossible to forget.

### 1.2 The repository root

One `index.md` at the root. An import reads it first, before it knows anything else, so the
modal can fill itself in one fetch.

```yaml
---
format: 2.0.0
author: Alice Smith
description: Electricity and magnetism, from charge to the electromagnetic wave.
order: [phy, math]
---
```

`format` is **semantic versioning**. The app rejects a repository whose **major** version it
does not speak, and accepts any minor or patch, so the format can gain fields without breaking
a course that does not use them. **Format 2.0.0** made the deck the unit: references moved
into decks and checkpoints into their own folder. A 1.x repository does not parse, and nothing
converts it.

A licence is the repository's own business (`LICENSE.md`); the format does not carry one.

### 1.3 `draft: true`

Front matter on any file excludes it from the `order` bijection, so half-written work can sit in
the tree without failing the check. A draft is never built and never reaches a learner.

### 1.4 Assets

`assets/` may sit beside any `index.md`, at any level. A reference resolves **relative to the
file that makes it**, exactly as in any markdown tool, and may climb levels, so a deck can use
a picture its section keeps:

```markdown
![An oscilloscope trace](assets/scope-trace.png)
![The circuit symbols](../assets/symbols.svg)
```

A path that climbs out of the repository is refused.

**The compiler inlines every asset as a `data:` URI.** Nothing may
*fetch* a side asset at runtime, and inlining keeps that exactly: offline works with no blob
URLs, no asset store and no service-worker involvement, and the diff already covers a changed
image because an asset is a file with a content hash like any other.

Allowed: `.png`, `.jpg`, `.jpeg`, `.webp`, `.svg`. **256 KB per asset**. The compile holds an
asset to this cap on its own too, whatever files it is given.

**A domain compiles to 32 MB of HTML at most**, pictures inlined. A picture inlines at every
place that shows it, so it counts once for each place. The compile refuses a domain past the
cap, and stops inlining there.

**A raster picture becomes a `data:` URI. An SVG becomes markup**, re-emitted through the
allowlist in §10. It is markup rather than a `data:` URI for one reason: a picture inside
`<img>` cannot take `currentColor`, and a figure has to follow the theme, light or dark. The
allowlist keeps geometry, type and paint, and writes a fresh document from them, so no author
byte is ever forwarded. Three colours are **sentinels**: `#000000` becomes the ink of the page,
`#1a1a1a` the graph ink, and `#0000ff` the accent. Type is the theme's, so `font-family` does
not survive.

A figure is one island in the app's page, so it cannot reach outside itself. The markup stops
at the end of the root `<svg>`: anything after it is dropped. A `class` does not survive,
because a class names the app's styles. Every `id` takes a prefix from a hash of the file, and
every `url(#…)` takes the same prefix, so an id never matches one of the page's or another
figure's. The root's `width` and `height` do not survive: the root takes its size from the
card and its shape from its `viewBox`. A root with no `viewBox` takes one from a plain numeric
`width` and `height`.

Base64 costs +33%, so assets are the one thing that makes a domain large in **bytes**. Diagrams
make it slow in **time**. The two scale independently.

### 1.5 `labels:` — a domain names its own things

The format's vocabulary is deliberately generic, because the engine is not academic: the same
tree serves a physics course, a codebase walkthrough, and a machine-operator manual. A domain
**renames what the interface calls things**, in its own `index.md`:

```yaml
# phy/index.md
labels:
  reference: Formula
  references: Formulary
  derivation: Proof
  checkpoint: Exam
  checkpoints: Exams
```

Every label is optional, and an omitted one falls back to the default below. The defaults are
generic on purpose: **a course that declares nothing reads neutrally**, and a course that wants
its own words says so once.

| Label | Default | A physics course | A codebase | A machine manual |
|---|---|---|---|---|
| `domain` | Domain | Subject | Service | Machine |
| `section` | Section | Branch | Area | System |
| `topic` | Topic | Chapter | Module | Assembly |
| `deck` | Deck | Deck | Walkthrough | Procedure |
| `card` | Card | Card | Step | Step |
| `exercise` | Exercise | Exercise | Question | Check |
| `solution` | Solution | Solution | Worked answer | Worked answer |
| `reference` | Reference | Formula | Contract | Specification |
| `references` | References | Formulary | Reference | Specifications |
| `derivation` | Derivation | Proof | Rationale | Standard |
| `checkpoint` | Checkpoint | Exam | Review | Sign-off |
| `checkpoints` | Checkpoints | Exams | Reviews | Sign-offs |

The last three columns are illustrations, not part of the format. Only the **Default** column is
normative.

A label changes **display only**. It never changes an id, a route, a storage key, or anything
the format parses. So renaming a label is always safe and never touches progress.

**The interface reads two labels today:** `references` and `checkpoints`, the names of the
Formulary and of the Exams. The other ten are part of the format, and the check accepts them,
but no surface shows them yet. A key that is not in the table is an error.

### Levels

| Level | Example | Is | Declared by |
|---|---|---|---|
| Repository | `github.com/me/stem` | one or more domains | its URL |
| Domain | `phy` | a build unit, one bundle | a directory + its `index.md` |
| Section | `eam` | a section, groups topics, holds the tier vocabulary | a directory + its `index.md` |
| Topic | `fields` | a chapter in the menu | a directory + its `index.md` |
| Deck | `field` | one concept | a `.md` file |
| Card | `1` | one step of a deck | a `::: card` block |
| Exercise | `q1` | one practice item | an `::: exercise` block |
| Reference | `ohms-law` | a result you look up | a `::: reference` block in a deck |
| Checkpoint | `2020` | an exam, with its own questions | a `.md` file under `checkpoints/` |

## 2. Decks and cards

A **deck** is one concept, taught across a short sequence of **cards** that the learner reads
one after another in the Course, the app's main scroll. A deck file has YAML front matter, then its cards
in order, then its exercises in order, then its references in order. The Course shows them in
that order: the content, the flashcards, the references.

````markdown
---
title: Solving a linear equation
---

::: card
A *linear equation* in one unknown has the form $a x + b = 0$.
:::

::: card
Solve it by isolating the unknown, undoing each operation in turn:

$$ 2x + 4 = 10 \implies 2x = 6 \implies x = 3 $$
:::

::: card
The graph of $y = a x + b$ is a straight line. The slope is $a$, and it meets the vertical
axis at $b$. For why, see [the intercept](reference:intercept).

```plot
x: { var: x, label: "$x$", from: 0, to: 8, ticks: 2 }
y: { label: "$y$", from: 0, to: 8 }
inputs:
  - { name: a, min: 0, max: 2, default: 0.5, step: 0.1, label: slope a }
  - { name: b, min: 0, max: 6, default: 1,   step: 0.5, label: intercept b }
draw:
  - curve: a * x + b
```
:::
````

- A deck's front matter takes `title` and `draft`, and nothing else (§13 lists the keys of each
  file).
- A `::: card` block is one **idea**, short (section 8). It holds prose, inline `$math$` and
  `$$display$$`, at most one ```` ```plot ```` graph, and an optional `![](assets/…)` picture.
- A card takes no id. The compiler ignores a word after `::: card`. It numbers the cards of a
  deck in order, from 1, and a `::: figure` takes a number in the same sequence: it is a card
  of its own.
- Text outside a block reaches no learner. The check refuses it, at its line.
- The cards read one after another; a card is seen when it crosses the reading line, and the
  deck is read when every card is seen.
- A card may link a **reference** with `[label](reference:id)`, rendered as a small Σ mark that
  opens the reference in its section's Formulary. This is the only inline link a card carries. There is
  no inline derivation link and no inline solution link.

---

## 3. Exercises and solutions

After its cards, a deck declares its **exercises**: the practice items that test the concept.
An exercise is reviewed as a **flashcard** and scheduled by FSRS, a spaced-repetition scheduler.

````markdown
::: exercise q1
Solve $3x - 6 = 9$ for $x$.

::: answer
$x = 5$. Add 6 to both sides, then divide by 3.
:::

::: solution
$$ 3x - 6 = 9 \implies 3x = 15 \implies x = 5 $$

Check: $3(5) - 6 = 9$. ∎
:::
:::
````

Every block is `::: kind` … `:::` with its content between the fences; blocks nest. An exercise
opens with `::: exercise <id>`. The id is optional: an exercise without one is `e1`, `e2`, … by
its place among the exercises of its file. Two exercises in one file cannot have the same id,
and a derived id counts, so a written `e2` and the second exercise without an id collide. Its
**prompt** is the text directly inside it; `::: answer` and `::: solution` are nested blocks; the
closing `:::` ends the exercise.

- The exercise **prompt** is the flashcard front: one question, one defensible answer.
- The **`::: answer`** block is the flashcard back: the answer plus at most one line of guidance.
  It is required: the check refuses an exercise without one.
  After flipping, the learner self-grades **fail / almost / perfect**, and FSRS reschedules the
  card.
- The **`::: solution`** block is optional: the full worked answer (the "exercise derivation"),
  shown under the answer. It is written as a clean log: one step to a line. A
  solution belongs to one exercise and is reached only from it. It is **not** a reference derivation
  and never appears under Reference.

An exercise belongs to its deck, so the deck is the concept it tests. There is no `explains`
edge: the relationship is the containment. A `reference:` link in the prompt, the answer or the
solution is a `reference` edge from the exercise. A `plot` block in an exercise is an error: the
app draws a plot only in a card or a figure.

---

## 4. References and derivations

A **reference** is a reusable result, declared in the deck that teaches it, after the deck's
exercises. The chapter, the section and the domain each collect the references of their decks,
and the domain's set is the **Formulary**. A reference is its statement plus an
optional **derivation** of why it holds.

````markdown
::: reference cramer
# Cramer's rule

For a system $A \vec{x} = \vec{b}$ with $\det A \ne 0$, the solution is

::: equation
x = \frac{\begin{vmatrix} p & b \\ q & d \end{vmatrix}}{\det A}, \quad
   y = \frac{\begin{vmatrix} a & p \\ c & q \end{vmatrix}}{\det A}.
:::

::: legend
$p$: the first equation's constant
$q$: the second equation's constant
:::

::: derivation
Replacing each column of $A$ by $\vec{b}$ and expanding gives the quotients above, as
[the determinant](reference:determinant) defines them. ∎
:::
:::
````

- `::: reference <id>` opens it. The **id** is required and unique in its domain, so a
  `[label](reference:id)` link from any deck reaches exactly one reference.
- The first line is its **title**, as `# Title`. It is required.
- The prose after the title is the **statement**. `::: equation` holds the bare equation in
  LaTeX, and `::: legend` says what each symbol stands for.
- The **`::: derivation`** block is optional, written as a log: one step to a line. The references it
  links are what the reference **rests on**: they form the `uses` graph, a **DAG**, which the
  app lists under the reference as "Rests on". The build rejects a cycle. Put each link after the
  clause that uses the result, as a marker: `Each drops a voltage by Ohm's law: $V_1 = I R_1$.[Ohm's law](reference:ohms-law)`.
  Do not write a sentence that lists them ("It rests on …"): the app already lists them.
- A reference is linked from a card, an exercise or another reference with
  `[label](reference:id)`. A link into another domain of the same repository is
  `[label](reference:/domain/id)`. The compiler resolves every link, and refuses one that names
  no reference, in this domain or in the other one, with its file and its line.

**The two derivations, never mixed:** a **derivation** answers "why is this reference true" and lives
under Reference; a **solution** answers "how do I solve this specific problem" and lives on its
exercise. Different blocks (`::: derivation` vs `::: solution`), different files, different homes.

**A solution is the exercise's own block, beside the answer, never inside it.** Close the answer
first:

```
::: exercise q2
Find the Thévenin equivalent the load sees.

::: answer
16 V in series with 4 Ω.
:::

::: solution
Remove the load. With the terminals open, …
:::
:::
```

One written inside `::: answer` used to build clean and render nothing at all, which is the worst
way to lose a piece of content: it says it is there and it is not. The build **rejects** it now,
naming the exercise.

---

## 5. Checkpoints

A **checkpoint** is an exam: a set of questions of its own, the way a university publishes one.
The app calls it an **Exam** through the domain's labels. It has nothing to do with the
flashcards: its questions are not a deck's exercises, and the schedule never sees them.

Checkpoints live in a domain's **`checkpoints/`** folder, beside the sections, in folders to
any depth: one folder per university, and one per year under it, for example. Every folder
has an `index.md` with its `title` and an optional `order`, validated like any container's.

````markdown
---
title: Physics, 2020
---

Answer every question without notes.

::: exercise q1
A 9 V source feeds a 3 Ω and a 6 Ω resistor in series. What is the current?

::: answer
1 A.
:::

::: solution
The resistances add: $9 / (3 + 6) = 1$ A.
:::
:::
````

- The front matter holds the **title**. The prose before the first question is the
  **instructions**.
- Each question is an `::: exercise` block, with an `::: answer` and an optional
  `::: solution`, exactly as a deck's. A checkpoint holds nothing else. A question without an id
  is `e1`, `e2`, … by its place, and two questions in one checkpoint cannot share an id.
- A checkpoint draws no `plot`, in its instructions or in a question.
- Two folders may each hold a checkpoint with the same id: a checkpoint's route carries its
  folders.

---

## 6. IDs and references

- **An id is the filename stem**, or the directory name for a container. Nothing declares one.
- **The domain id is a hash** over the repository's canonical URL plus the domain's directory
  name (`core::model::domain_id`):
  - `https://github.com/Alice/stem.git` + `phy` → `github.com/alice/stem/phy` → `9c8f9b9e`
  - The same repository's `math` → `0d0d4df6`. One repository, several domains, no collision.
  - Two repositories may both hold a `phy`. They never share a progress row.
  - Every device derives the same id from the same pair, so identity never syncs.
  - The **ref is not in it**: a branch and a tag are one domain at two versions.
  - The directory name is taken **verbatim**, unlike the URL. A host treats a repository name
    case-insensitively, so the URL folds; a git tree does not, so `Phy/` and `phy/` are two
    directories and stay two domains.
- **Renaming a file or a directory moves its id**, and everything under it. The refresh carries
  the progress across: the renamed files move with their content hashes unchanged, which is the
  rename the diff detects (see the refresh, below). **Reordering costs nothing**,
  because `order` names ids and never touches a path.
- **Uniqueness.** The global id comes from the containing chain, but the check asks more than
  that:
  - a section, a topic and a deck id are **unique per kind in the domain**. Two decks called
    `solving` in two topics of one domain are an error, and so are two topics called `basics`
    in two sections. A deck and a topic may share an id: the rule is per kind.
  - a reference id is unique in the domain (§4).
  - an exercise id and a figure id are unique in their file.
  - a checkpoint id is free: its route carries its folders, so two folders may each hold a
    `2020`.
- The build resolves the global id from the containing chain, under the domain id:
  - deck: `<domain-id>/alg/linears/solving`
  - card: `<domain-id>/alg/linears/solving#1` (cards are numbered in order, and take no id)
  - exercise: `<domain-id>/alg/linears/solving#q1`
  - reference: `<domain-id>/cramer` (declared in a deck, but domain-scoped, so a link needs no
    path)
  - checkpoint: `<domain-id>/checkpoints/mit/2020`, and its question `…/2020#q1`
- **References are browser-style**, relative to the language root, language-free:
  - to a reference (a `reference:` link in a card, an exercise or a reference):
    `reference:cramer` (same domain) or `reference:/physics/…` (another).
  - to a topic (`requires`): the topic path, no fragment, e.g. `fundamentals` (same section)
    or `/math/alg-basics/fundamentals`. The check refuses a `requires` entry that names no
    topic, and a cycle in the `requires` graph.
- Solutions and answers need no id or reference: they are sub-blocks of their exercise.
- **A cross-domain reference works inside its repository, and nowhere else.** `/math/...` from
  a card in `phy` resolves at build, because both domains are in the tree the build walks. A
  reference to a domain in **another repository** cannot resolve: the other repository may not
  be installed, and its id is a hash the author cannot write down. The check **rejects** such a
  reference, in a `reference:` link and in `requires`, rather than emitting a dangling edge. This is the reason to keep related domains
  together. No reference crosses the two domains today, but physics will need mathematics:
  complex numbers for alternating current, and calculus for the time constants.
- Global ids are permanent in practice. Moving a folder renames its content, and the refresh
  **detects that rename** by matching content hashes across the two paths, exactly as git infers
  one, so the learner's progress follows the content rather than dying with the old name.
  **Renaming a domain folder is the same act one level up**: every file under it moves with its
  content hash unchanged, so the refresh carries the whole domain's progress to the new id.

---

## 7. Tiers

`tier` is an ordered, content-defined audience or difficulty band. The **section** declares the
vocabulary, in order (`tiers: [entrance, university]`); each **topic** picks one (`tier:
entrance`). A topic may leave out `tier`. A `tier` that is not in its section's `tiers` is an
error. Values are opaque to the tools, so the engine is not school-specific:
- exam prep: `entrance`, `university`, `graduate`.
- vocational or factory: `induction`, `operator`, `certified`, `expert`.

> **Nothing reads them yet.** The build carries `tier` into `BundleTopic.tier` and `tiers` into
> `BundleSection.tiers`, and the app drops both: `data::TopicView` has no `tier` field. They are
> carried for a planned overlay of learning tracks, so a course may declare them, but no
> surface filters on them today.

---

## 8. Sizing: one idea per card

A card is **one idea, short**. The Course is a scroll, so a card has no fixed frame, never
paginates, and never scrolls inside itself: it is as long as its prose. What keeps it short is
the author, not a frame. A card that holds two ideas is two cards; a derivation that grows
belongs in a reference.

---

## 9. Language

**The content is not internationalised.** A domain declares the one language it is written in:

```yaml
title: Physics
lang: en
```

`lang` is a **BCP 47 tag**: `en`, `pt-BR`, `zh-Hans`. It is a **declaration, never a
selection**. It is required. While a domain is open, the app puts it on the content column
(`<main lang>`), so a screen reader reads the content in that language and the interface
around it in the interface's language.
The tag does nothing else today. The check does not test that it is a valid BCP 47 tag; it
requires only that it is not empty.

**A course in another language is another domain.** It carries its own id, its own progress and
its own row in the selector, whether it sits in the same repository or a different one:

```markdown
---
lang: pt-BR
title: Física
---
```

Its directory is its id, for example `phy-pt/`.

There are no `[lang]/` directories, no translation files, no per-file fallback, and no content
language for the learner to pick. Picking a domain already is that choice. Translating a course
is forking it.

The **interface** language is a separate thing and is untouched. The app ships its own chrome
strings through `rust-i18n`. It ships English only for now, and the locale system stays, so a
new interface language is one YAML file.

---

## 10. The block grammar

**Nine blocks and one fenced language, `plot`.** A block is `::: kind` … `:::`, with an optional
id after the kind, and blocks nest **one level**: an `answer` or a `solution` in an `exercise`,
an `equation`, a `legend` or a `derivation` in a `reference`. The check refuses a block that
opens two levels down, at its line. A live diagram is a fenced code block, not a `:::` block.
The compiler reads the id of an exercise, a figure and a reference, and ignores a word after
any other block.

| Block | Where | Is |
|---|---|---|
| `::: card` | a deck | one step: one idea |
| `::: figure` | a deck | a diagram with a caption, as a card of its own |
| `::: exercise <id>` | a deck, or a checkpoint | a flashcard in a deck; a question in a checkpoint |
| `::: answer` | inside an exercise | the flashcard back, or the question's answer |
| `::: solution` | inside an exercise | the worked answer, under the answer |
| `::: reference <id>` | a deck | a result to look up, with its `# Title` |
| `::: equation` | inside a reference | the bare statement, in LaTeX, no `$$` |
| `::: legend` | inside a reference | what each symbol stands for |
| `::: derivation` | inside a reference | why it holds; its links are what it rests on |

The equation and the legend are blocks rather than front-matter fields, because each carries
**content that belongs with the prose**: a LaTeX equation in a YAML block scalar was awkward to
write and easy to break, and a legend's meanings are prose.

```markdown
::: equation
I = \frac{V}{R} \qquad V = I R \qquad R = \frac{V}{I}
:::

::: legend
$I$: current, in amperes
$V$: voltage across the conductor, in volts
$R$: resistance, in ohms
:::
```

A legend is one `symbol: meaning` per line. The symbol renders as inline MathML. The meaning is
prose: write its math in `$…$`, as in a card. Only the first `:` separates them, so a meaning may
contain one.

### Drawing: one fenced language, and SVG

MathML cannot draw, and a picture is one of two things.

| To draw | Write |
|---|---|
| Anything **live**: a graph that answers a slider | ```` ```plot ````, §10.1 |
| Anything **still**: a schematic, a photograph, a figure | `![caption](assets/x.svg)`, §1.4 |

A still picture is an asset, drawn however its author likes, and **re-emitted on import**. The
app does not filter the author's markup: it reads the file and writes a new one from the
elements and the attributes its allowlist knows, escaping every value itself. An element it does
not know takes its whole subtree with it, so a `<script>` cannot come back as text. The prose
is **Markdown with raw HTML off**, so an author's `<script>` in a card renders as the text it
is.

**The MathML is re-emitted the same way.** The LaTeX renderer writes the text of `\text{…}`
and `\operatorname{…}` into its MathML as it stands, so its output is author markup too. The
compiler reads it and writes a new document from the MathML elements the renderer writes for
valid LaTeX (`math`, `mrow`, `mi`, `mn`, `mo`, `mtext`, `mspace`, `msup`, `msub`, `msubsup`,
`mfrac`, `msqrt`, `mroot`, `mover`, `munder`, `munderover`, `mtable`, `mtr`, `mtd`) and their
presentation attributes, each value held to a closed grammar. An element it does not know,
such as an `<img>` or a `<style>` inside a `\text{…}`, is dropped with its subtree, and
anything after the root `</math>` is dropped. No `class` and no `style` survives, so `\color`
draws in the ink of the page. Math nested deeper than **256 levels** is refused.

**A plain link** in prose stays a link only to `https:` or `mailto:`, and opens with
`rel="noopener noreferrer"`. Any other target, `http:` and a `#fragment` included, reads as
the link's text.

### The two link forms

Both resolve at compile, and both are rejected, with their file and their line, if they
dangle:

| Written | Reaches |
|---|---|
| `[Ohm's law](reference:ohms-law)` | a reference in this domain, rendered as a small Σ mark; the label names it for a screen reader and in its tooltip |
| `[Ohm's law](reference:/phy/ohms-law)` | a reference in another domain of this repository |
| `[Figure](figure:gravity-field)` | a `::: figure` in the same deck, rendered as its number |

An image is the third form, and it is **not** a link to content: `![caption](assets/x.png)`
inlines an asset (§1.4).

## 10.1 The `plot` block

One fence, and everything that moves. A plot is an **expression in one free variable**, with
named values bound to sliders, drawn as SVG by the app and re-evaluated as the learner drags.

**A plot goes in a `::: card` or a `::: figure`, one in each.** The app draws nothing else, so
the check refuses a plot in an exercise, an answer, a solution, a reference, a derivation or
a checkpoint, and a second plot in one card.

### The shape

| Key | Is |
|---|---|
| `x`, `y` | the two axes |
| `inputs` | the sliders, each a named value |
| `let` | named sub-expressions, in order |
| `draw` | the marks, back to front |

### An axis

```yaml
x: { var: t, label: "$t$ in seconds", from: 0, to: 10, scale: linear, ticks: 2, grid: true }
```

| Key | Is |
|---|---|
| `var` | the **free variable** a curve is a function of. On `x` only. |
| `label` | axis text. Takes `$math$`, set as italic serif: SVG cannot hold MathML, so a symbol reads and a fraction does not. |
| `from`, `to` | the extent |
| `scale` | `linear` (the default) or `log`, which is what makes a Bode plot possible |
| `ticks` | the spacing, or absent for none. On a `log` axis, a tick at every power of ten. A tick is a short mark with no number: put the values a learner must read in the prose or a `point` label |
| `grid` | faint rules at the ticks |

### An input

```yaml
inputs:
  - { name: f, min: 0.5, max: 3, default: 1, step: 0.1, label: frequency }
```

`name` is the identifier every expression may use; `label` is what the learner reads. **With no
inputs the plot is still**, which is all a picture-only graph needs.

### Limits

The app draws a plot in the learner's browser, so the check holds every plot to these:

- every number (`from`, `to`, `ticks`, `min`, `max`, `default`, `step`, and each value of a
  `points` mark) is finite: not `.nan`, not `.inf`;
- `step` and `ticks` are above zero;
- **16** inputs, **32** `let` bindings and **32** marks at most;
- **1000** data points at most, over all the `points` marks of the plot;
- an expression is **512 bytes** at most. Name its parts in `let`.

### `let`

```yaml
let:
  w:   2 * pi() * f
  env: exp(-d * t)
```

Evaluated in order. Each is visible to the ones after it and to every mark, so a thing is said
once. It is a **mapping**, and its order is part of its meaning, so it is read and written in
the order the author wrote it.

### The marks

| Mark | Draws |
|---|---|
| `curve` | `y = f(x)` |
| `param` | a parametric pair, `(x(s), y(s))` |
| `polar` | `r(θ)` |
| `point` | one marked, optionally labelled point |
| `points` | a data series |
| `hline`, `vline` | a reference line |
| `area` | a filled region |

```yaml
draw:
  - curve: sin(w * t)                                   # the short form
  - curve: { is: env, accent: true, dash: true, label: envelope }
  - curve: { is: 1 / t, over: [0.5, 10] }               # a restricted domain
  - param: { var: s, over: [0, 2 * pi()], x: cos(s), y: sin(s) }
  - polar: { var: a, over: [0, 2 * pi()], r: 1 + cos(a) }
  - point: { at: [2, 1], label: "$t_0$" }
  - points: [[0, 0], [1, 0.8], [2, 1.4]]
  - hline: { at: 0.707, label: "$-3$ dB", dash: true }
  - vline: { at: 2.5 }
  - area:  { under: sin(w * t), over: [0, pi()] }
  - area:  { between: [sin(t), cos(t)], over: [0, 1] }
```

Three style keys are shared by every mark: **`accent`**, the one colour beside the ink that
a figure may carry; **`dash`**; and **`label`**. Each mark draws all three, `points`
and `area` included. And **`over`** means one thing everywhere: the extent a mark is drawn
across.

**A restricted domain is how a plot goes piecewise.** Two `curve` marks with adjoining `over`
ranges draw a square wave or a diode curve, and no new syntax is needed.

### The expression language

Every expression is [`fasteval`](https://docs.rs/fasteval): the operators `+ - * / % ^`, the
comparisons `< <= > >= == != && ||` which yield `1` or `0`, and the functions `sin cos tan asin
acos atan sinh cosh tanh abs sign min max floor ceil round int log pi() e()`.

Two of its rules surprise an author:

- **`log(x)` is base 10.** `log(b, x)` takes the base first, so the natural logarithm is
  `log(e(), x)`.
- **A letter right after a number is a unit suffix.** `2k` is 2000, and `2m` is 0.002: `p n u
  m k K M G T` scale by powers of ten. It is never a product, so write `2 * k` for two times
  `k`.

Three more are ours, because the workaround for each is ugly enough to bite an author:

| Ours | Else it reads |
|---|---|
| `exp(x)` | `e()^x` |
| `sqrt(x)` | `x^0.5` |
| `if(c, a, b)` | `(c)*a + (1-(c))*b` |

Because a comparison yields `1` or `0`, `if` composes: `if(t < 0, -1, 1)` is a square wave.

### A worked example

```yaml
x: { var: t, label: "$t$ in seconds", from: 0, to: 10, ticks: 2, grid: true }
y: { label: "$V$", from: -1.2, to: 1.2 }

inputs:
  - { name: f, min: 0.2, max: 2,   default: 0.6,  step: 0.1,  label: frequency }
  - { name: d, min: 0,   max: 0.5, default: 0.2,  step: 0.05, label: damping }

let:
  w:   2 * pi() * f
  env: exp(-d * t)

draw:
  - area:  { under: env * sin(w * t), over: [0, 1 / (2 * f)] }
  - curve: env * sin(w * t)
  - curve: { is: env,  accent: true, dash: true, label: envelope }
  - curve: { is: -env, accent: true, dash: true }
  - hline: { at: 0 }
  - point: { at: [1 / (4 * f), env], label: first peak }
```

Two sliders; the wave, its envelope drawn both ways, the first half-cycle shaded, and a
labelled peak that **moves with the sliders, because its coordinates are expressions**.

### What a plot deliberately does not do

| Not this | Why |
|---|---|
| implicit relations, `x^2 + y^2 = 9` | needs marching squares, and a circle is a `param` |
| derivatives and integrals as operators | a symbolic layer we would own forever |
| regressions and statistics | not what a card is for |
| points the learner drags | a card is for reading, not authoring |

---

## 11. bundle.yaml (generated, per domain, localized)

The compiler renders every card, exercise, solution, reference, derivation and checkpoint to
HTML, resolves ids and references to global paths, validates, and emits one bundle per domain
(**version 6**).

**The learner's device holds it as JSON in `IndexedDB`**, keyed by the domain id, written by the
import worker and parsed when the domain is opened. The YAML below shows the same structure, so
an author can read what a learner will get:

```yaml
version: 6
lang: en
author: Alice Smith                # the domain's own, else the repository's
domain: { id: 9c8f9b9e, title: Mathematics }   # the id is the hash, never the folder
sections:
  - id: alg
    title: Algebra
    tiers: [entrance, university]
    topics:
      - id: linears
        title: Linear equations
        tier: entrance
        decks: [solving, matrix-form]     # lesson order
decks:
  "math/alg/linears/solving":
    title: Solving a linear equation
    cards:                                 # rendered, in order
      - { rev: b3f1a9c2, html: "<p>A <em>linear equation</em> …</p>" }
    exercises:
      - id: q1
        rev: 9a01ffcd
        prompt:   "<p>Solve <math>…</math> for <math>…</math>.</p>"
        answer:   "<p><math>…</math>. Add 6, then divide by 3.</p>"
        solution: "<p><math display=\"block\">…</math>Check: … ∎</p>"
    references: ["math/intercept"]         # the ones this deck declares, in order
references:
  "math/intercept":
    title: The intercept
    rev: c11a77f0
    reference: "<math display=\"block\">…</math>"    # the equation
    legend: [{ symbol: "<math>…</math>", meaning: "the intercept" }]
    html: "<p>A line meets the vertical axis …</p>"   # the statement
    derivation: "<p>Substituting …∎</p>"                # optional
checkpoints:
  - kind: folder
    id: mit
    title: MIT
    entries:
      - kind: checkpoint
        id: "2020"
        title: Physics, 2020
        rev: 7d20aa13
        html: "<p>Answer every question without notes.</p>"   # the instructions
        questions:
          - { id: q1, rev: 1f0e33aa, prompt: "…", answer: "…" }
edges:
  - ["math/alg/linears", requires, "math/alg/fundamentals"]   # topic → topic
  - ["math/quadratic-formula", uses, "math/discriminant"]     # a derivation's link
  - ["math/alg/linears/solving#3", reference, "math/intercept"] # a card's link
```

- `sections/topics` give the menu structure: sections, topics (with tier, `requires`, and
  ordered `decks`).
- `decks` are keyed by global id, each with its `cards` (rendered HTML, in order), its
  `exercises` (each: `prompt`, `answer`, optional `solution`, all rendered HTML, plus a `rev`),
  and the ids of its `references`. The HTML is self-contained (prose + MathML + inline themed
  SVG), inlined by the app, no side assets.
- `references` are keyed by global id: the rendered equation, legend, statement, an optional
  `derivation`, and a `rev`.
- `checkpoints` is the tree of `checkpoints/`: each entry a `folder` with its entries, or a
  `checkpoint` with its instructions and its own `questions`.
- `edges` are resolved triples: `requires` (topic→topic), `uses` (reference→reference, from a
  derivation), and `reference` (anything else that links a reference).
- `rev` is a per-unit content hash for cache-busting, offline updates, and sync.

### Root index (`index.yaml`)

Stemin's own build tool also writes a root `index.yaml` cataloguing what it produced. **The app
never reads one.** It knows what exists from the repositories on its
own list and the artifacts on the device.

---

## 12. Edges

Three edges, emitted as triples `[from, predicate, to]`. The inverse is derived by the app, never
stored:
- `requires`: **topic → topic**, from a topic's own `index.md`. The menu's prerequisite
  structure. May cross sections and domains inside one repository; a reference into another
  repository is rejected (§6).
- `uses`: **reference → reference**, from the `reference:` links in a reference's derivation.
  The "rests on" DAG of the Formulary.
- `reference`: **card, exercise or statement → reference**, from any other `reference:` link.
  "This concept uses that result." (inverse: where a reference is used.)

Every `requires`, `uses` and `reference` target is validated at compile, inside one domain and
across the domains of the repository. The check rejects a target that is not there, a target
in another repository, a cyclic `requires` graph and a cyclic `uses` graph.

---

## 13. Authoring workflow

1. Make the topic directory, `<domain>/<section>/<topic>/`, with an `index.md`: its `title`,
   `tier`, any `requires`, and an `order` of the decks it will hold.
2. Write each **deck** as `<deck>.md`: front matter (`title`), then `::: card` blocks (the
   concept, one idea each), then `::: exercise` blocks (each with an `::: answer` and an
   optional `::: solution`), then `::: reference <id>` blocks (each with a `# Title`, an
   `::: equation`, a `::: legend`, the statement and an optional `::: derivation`). Add the
   deck's id to the topic's `order`. Link a reference from anywhere with
   `[label](reference:id)`.
3. Add the topic's id to the section's `order`, and the section's id to the domain's. Declare the
   section's `tiers`.
4. Write **checkpoints**, if the domain has any, under `<domain>/checkpoints/`: a folder per
   source, an `index.md` in each, and one file per checkpoint with its own questions.
5. Run **`stemin check`**. It runs the compile the browser runs on an import, and writes
   nothing, so a repository that passes it imports and one that fails it does not. It reports
   every fault with its file and, where it has one, its line. This is the gate a content
   repository runs in CI.
6. Run the build, if you want to read the output. It renders everything to HTML and emits
   `bundle.yaml` (version 6).

Anything half-written carries `draft: true` and is skipped by both (§1.3).

### What `stemin check` refuses

The structural rules come first. While one of them fails, nothing renders.

- the root: no `index.md`, no `format:`, or a major version this build cannot read;
- the disk: a symbolic link, a device, a pipe or a socket (§1);
- the tree: the `order` bijection, `order` naming a draft or one id twice, an empty
  container, a directory where only files belong, a reserved id, two sections, topics or decks
  of one domain with one id, a path more than 16 levels deep, a `.md` file over 512 KB;
- front matter: YAML that will not parse, a key its file does not take, a domain without
  `lang`, a label not in §1.5. The keys are:

  | File | Keys |
  |---|---|
  | the root | `format`, `author`, `description`, `order` |
  | a domain | `title`, `lang`, `author`, `labels`, `order`, `draft` |
  | a section | `title`, `tiers`, `order`, `draft` |
  | a topic | `title`, `tier`, `requires`, `order`, `draft` |
  | a checkpoint folder | `title`, `order`, `draft` |
  | a deck, a checkpoint | `title`, `draft` |

- blocks: a word outside the grammar, a block never closed or closed twice, a block in the
  wrong place, a block two levels down, text outside a block (outside a checkpoint's instructions), an exercise without
  an answer, two exercises or two figures in one file with one id, a reference without an id or
  a `# Title`, two references of one domain with one id, a legend row without `:`;
- plots: a plot that will not parse, a number that is not finite, a `step` or `ticks` of zero
  or less, a plot past a limit of §10.1, a plot outside a card or a figure, a second plot in a
  card;
- the graph: a `tier` its section does not list, a `requires` entry that names no topic or
  leaves the repository, a `requires` cycle;
- assets: a file outside `assets/`, a type the format does not carry, a file over 256 KB.

Then the compile, which carries on past each fault and names them all:

- LaTeX that will not render, in prose, an equation or a legend symbol, or that nests deeper
  than 256 levels;
- a `reference:` link that names no reference, here or in another domain of the repository, or
  that leaves the repository; a `figure:` link that names no figure of its deck; a `uses` cycle;
- a picture that is not in the repository, sits on another site, is over 256 KB, or is an SVG
  the allowlist cannot read;
- a domain that holds no deck, or that compiles to more than 32 MB of HTML.
