//! What a content repository can get wrong, and where.
//!
//! Every failure carries a file and, where the parser knows one, a line. A
//! content author reads these in CI, so each message says what is wrong and what
//! to do, never just which invariant tripped.

use std::path::PathBuf;

/// Where a failure sits. A line of `0` means the file as a whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct At {
    pub file: PathBuf,
    pub line: usize,
}

impl At {
    /// A failure in a file, with no line to point at.
    #[must_use]
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            file: path.into(),
            line: 0,
        }
    }

    /// A failure at one line, counted from 1.
    #[must_use]
    pub fn line(path: impl Into<PathBuf>, line: usize) -> Self {
        Self {
            file: path.into(),
            line,
        }
    }
}

impl std::fmt::Display for At {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.file.display())
        } else {
            write!(f, "{}:{}", self.file.display(), self.line)
        }
    }
}

/// One thing wrong with a content repository.
#[derive(Debug, thiserror::Error)]
#[error("{at}: {kind}")]
pub struct Fault {
    pub at: At,
    pub kind: FaultKind,
}

impl Fault {
    /// Report a fault at a whole file.
    #[must_use]
    pub fn file(path: impl Into<PathBuf>, kind: FaultKind) -> Self {
        Self {
            at: At::file(path),
            kind,
        }
    }

    /// Report a fault at one line.
    #[must_use]
    pub fn line(path: impl Into<PathBuf>, line: usize, kind: FaultKind) -> Self {
        Self {
            at: At::line(path, line),
            kind,
        }
    }
}

/// The kinds of fault, one per rule `stemin check` enforces.
#[derive(Debug, thiserror::Error)]
pub enum FaultKind {
    /// The repository's root `index.md` is absent.
    #[error("no `index.md` at the repository root: every content repository needs one")]
    NoRoot,

    /// `format:` is absent, unparseable, or a major version this build cannot read.
    #[error(
        "this repository targets format {found}, and this build reads {readable}.x: \
         update the app, or pin the repository to a release that speaks {found}"
    )]
    FormatMajor { found: String, readable: u64 },

    /// `format:` is missing or not semantic versioning.
    #[error("`format:` must be a semantic version like `2.0.0`, and {0}")]
    FormatMalformed(String),

    /// The YAML front matter will not parse.
    #[error("front matter will not parse: {0}")]
    BadFrontMatter(String),

    /// A `:::` block was opened and never closed.
    #[error("block `{0}` is never closed: add a `:::` line")]
    UnclosedBlock(String),

    /// A `:::` line closes nothing.
    #[error("`:::` closes nothing here")]
    UnopenedBlock,

    /// A block name the grammar does not hold.
    #[error("`{word}` is not a block: the grammar is {}", crate::block::GRAMMAR)]
    UnknownBlock { word: String },

    /// A block opened inside a block that already sits inside another.
    #[error(
        "`::: {0}` opens two blocks deep, and a block nests one level at most \
         (an `answer` in an `exercise`, a `legend` in a `reference`): \
         close the block above it first"
    )]
    NestedTooDeep(String),

    /// A block that cannot sit where it does.
    #[error("`{block}` cannot sit inside `{parent}`")]
    MisplacedBlock { block: String, parent: String },

    /// A block that must sit inside another, at the top level.
    #[error("`{0}` cannot sit on its own: it belongs inside the block it explains")]
    OrphanBlock(String),

    /// Two files claim one id.
    #[error("`{id}` is already the id of a {kind}, at {other}: ids are unique per kind")]
    DuplicateId {
        id: String,
        kind: &'static str,
        other: String,
    },

    /// `order` lists a name with no file behind it.
    #[error("`order` names `{0}`, and no file or directory here carries that id")]
    OrderMissing(String),

    /// A file is present and `order` does not list it.
    #[error(
        "`{0}` is here and `order` does not list it: add it to `order`, \
         or mark the file `draft: true`"
    )]
    OrderUnlisted(String),

    /// `order` names one id more than once.
    #[error("`order` names `{0}` more than once")]
    OrderRepeats(String),

    /// A directory sitting where only files belong.
    #[error(
        "`{0}` is a directory, and only files belong at this depth, \
         so nothing can ever reach it: move it up a level"
    )]
    Unreachable(String),

    /// A ```` ```plot ```` fence the app could not read.
    #[error("this `plot` block will not parse, so it would draw nothing: {0}")]
    BadPlot(String),

    /// A legend line the parser cannot split.
    #[error(
        "`{0}` is not a legend row: each line reads `$symbol$: what it means`, \
         and this one has no `:`"
    )]
    BadLegendRow(String),

    /// `order` names something that is there, and marked half-written.
    #[error(
        "`order` names `{0}`, which is marked `draft: true`: \
         take it out of `order`, or take the draft mark off"
    )]
    OrderNamesDraft(String),

    /// A `::: reference` block with no id after the word.
    #[error(
        "`::: reference` needs an id, like `::: reference ohms-law`: \
         a `reference:` link names a reference by it"
    )]
    ReferenceNoId,

    /// A `::: reference` block with no `# Title` line.
    #[error(
        "reference `{0}` has no `# Title` line: the title is what the learner \
         looks the reference up by"
    )]
    ReferenceNoTitle(String),

    /// A reserved id used as a content id.
    #[error("`{0}` is reserved, because the app routes on it: rename this one")]
    ReservedId(String),

    /// A link or a `requires` entry that resolves to nothing.
    #[error("`{target}` names no {kind} in {scope}")]
    Dangling {
        target: String,
        kind: &'static str,
        scope: String,
    },

    /// A reference that points outside this repository.
    #[error(
        "`{0}` points outside this repository, and a reference cannot cross one: \
         move that content here, or drop the link"
    )]
    ForeignReference(String),

    /// A cycle in `requires` or `uses`.
    #[error("`{predicate}` is cyclic: {}", .path.join(" -> "))]
    Cycle {
        predicate: &'static str,
        path: Vec<String>,
    },

    /// An asset the format does not carry.
    #[error("`{0}` is not an asset this format carries: use .png, .jpg, .jpeg, .webp or .svg")]
    AssetType(String),

    /// An asset over the per-file cap.
    #[error("`{path}` is {size} bytes, over the {cap}-byte cap: shrink it before committing it")]
    AssetTooBig { path: String, size: u64, cap: u64 },

    /// A `.md` file over the per-file cap.
    #[error(
        "this file is {size} bytes, over the {cap}-byte cap on a `.md` file: \
         split it into more decks"
    )]
    TextTooBig { size: u64, cap: u64 },

    /// A file below more folders than the walk reads.
    #[error(
        "this sits more than {0} levels below the repository root, and nothing is read \
         that deep: move it up"
    )]
    PathTooDeep(usize),

    /// A symbolic link in a repository on disk.
    #[error(
        "`{0}` is a symbolic link, and the check does not follow one: \
         put the file itself in its place"
    )]
    Symlink(String),

    /// A device, a pipe or a socket in a repository on disk.
    #[error("`{0}` is not a regular file or a directory: take it out of the repository")]
    NotAFile(String),

    /// An asset outside an `assets/` directory.
    #[error("`{0}` sits outside an `assets/` directory: every asset belongs in one")]
    AssetMisplaced(String),

    /// A container with nothing in it.
    #[error("this {0} holds nothing: give it children, or take it out")]
    EmptyContainer(&'static str),

    /// A domain without a language.
    #[error("`lang:` is missing or malformed: a domain declares one BCP 47 tag, like `en`")]
    BadLang,

    /// An exercise with no flashcard back.
    #[error(
        "this exercise has no `::: answer`: the answer is the back of its flashcard, \
         and every exercise needs one"
    )]
    NoAnswer,

    /// A front-matter key the format does not read.
    #[error("`{key}` is not a field of a {kind}: a {kind} takes {allowed}")]
    UnknownKey {
        key: String,
        kind: &'static str,
        allowed: String,
    },

    /// A label the format does not know.
    #[error("`{0}` is not a label: the labels are {names}", names = crate::front::label_names())]
    UnknownLabel(String),

    /// Two exercises, or two figures, in one file with one id.
    #[error("`{id}` is already the id of another {what} in this file, at line {line}: rename one")]
    DuplicateLocal {
        id: String,
        what: &'static str,
        line: usize,
    },

    /// Text outside any block, which no learner sees.
    #[error("this text sits outside any block, so the app never shows it: {0}")]
    LooseText(&'static str),

    /// A `plot` fence where the app does not draw one.
    #[error(
        "a `plot` block draws only in a `card` or a `figure`, and this one sits in {0}, \
         so the app never draws it: move it to a card"
    )]
    PlotPlacement(String),

    /// A second `plot` fence in one card.
    #[error(
        "this `{0}` already holds a `plot` block, and a card draws only one: \
         put the second plot in a card of its own"
    )]
    TwoPlots(String),

    /// A topic's tier that its section does not declare.
    #[error(
        "tier `{tier}` is not in the section's `tiers`, {tiers}: use one of them, or add it there"
    )]
    TierUnknown { tier: String, tiers: String },

    /// What the compiler found as it rendered: math, a picture, a link.
    #[error("{0}")]
    Render(String),

    /// The filesystem would not cooperate.
    #[error("cannot read: {0}")]
    Io(String),
}
