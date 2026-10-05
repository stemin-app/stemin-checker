//! Serde types for the content contract.
//!
//! A theme, the generated `bundle.yaml` plus `index.yaml`, and one row of a
//! learner's state. The **authoring** front matter lives in `stemin_format::front`,
//! because the checker and the app both read it and neither renders.
//! Shapes follow CONTENT-MODEL.md.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// Ids live in the `format` crate, because the checker and the app both need them
// and neither can carry a renderer. Re-exported so callers keep one import.
pub use stemin_format::id::{GlobalId, canonical_repo_url, domain_id, repo_id};
// The `plot` specification lives in the format crate too: it is the one fenced
// language a third party writes, and the app draws it without a renderer.
pub use stemin_format::plot::{Input as PlotInput, Mark, Plot, Style as MarkStyle};

// ---------------------------------------------------------------------------
// Themes
// ---------------------------------------------------------------------------

/// A theme: a name, a logo, the fonts, and the colours of each mode.
#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    pub name: String,
    #[serde(default)]
    pub logo: Option<String>,
    pub fonts: ThemeFonts,
    /// Colour modes (`light`, `dark`) mapping CSS token names to values.
    #[serde(default)]
    pub colors: BTreeMap<String, BTreeMap<String, String>>,
}

/// The theme's font roles. `content` is the book serif; `math` is the MATH-table
/// font for native MathML. Both ship as `@font-face`.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeFonts {
    pub content: String,
    #[serde(default)]
    pub math: Option<String>,
    #[serde(default)]
    pub chrome: Option<String>,
    #[serde(default)]
    pub mono: Option<String>,
}

// ---------------------------------------------------------------------------
// Generated output: bundle.yaml (per domain, localized)
// ---------------------------------------------------------------------------

/// The current bundle schema version.
pub const BUNDLE_VERSION: u32 = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub version: u32,
    pub lang: String,
    /// What this domain calls things, over the generic defaults
    /// (CONTENT-MODEL.md §1.5). Display only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// Who wrote this domain: its own `author`, else the repository's. The
    /// selector reads it beside the title ("Electromagnetism, by …"), so a
    /// learner with several repositories can tell two alike domains apart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub domain: DomainRef,
    pub sections: Vec<BundleSection>,
    pub decks: BTreeMap<GlobalId, DeckDef>,
    /// Every reference in the domain, from the decks that declare them. Each
    /// deck lists its own, in order, in [`DeckDef::references`].
    pub references: BTreeMap<GlobalId, ReferenceDef>,
    /// What `checkpoints/` holds: folders and checkpoints, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checkpoints: Vec<CheckpointEntry>,
    pub edges: Vec<Edge>,
}

/// Domain id and title in the bundle header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainRef {
    pub id: String,
    pub title: String,
}

/// A section (section) group in the bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleSection {
    pub id: String,
    pub title: String,
    pub tiers: Vec<String>,
    pub topics: Vec<BundleTopic>,
}

/// A topic (a chapter in the menu) in the bundle: its decks, in order.
/// Everything a chapter shows beyond its decks is collected from them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleTopic {
    pub id: String,
    pub title: String,
    pub tier: String,
    #[serde(default)]
    pub decks: Vec<String>,
}

/// One entry under a domain's `checkpoints/`: a folder, or a checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CheckpointEntry {
    Folder(CheckpointFolderDef),
    Checkpoint(CheckpointDef),
}

/// A folder of checkpoints: its title, and what it holds, in order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointFolderDef {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub entries: Vec<CheckpointEntry>,
}

/// One checkpoint: its instructions and its own questions. The questions are
/// not flashcards, and the schedule never sees them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointDef {
    pub id: String,
    pub title: String,
    pub rev: String,
    /// The instructions, the prose before the questions.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html: String,
    pub questions: Vec<ExerciseDef>,
}

/// A deck: the unit an author writes. Its cards, its exercises (the
/// flashcards) and its references, each in order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckDef {
    pub title: String,
    pub cards: Vec<CardDef>,
    pub exercises: Vec<ExerciseDef>,
    /// The global ids of the references this deck declares, in order. Their
    /// content sits in [`Bundle::references`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<GlobalId>,
}

/// One card: rendered HTML (prose + MathML + themed SVG), and an optional
/// interactive plot the app draws live.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardDef {
    pub rev: String,
    pub html: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plot: Option<Plot>,
}

/// One exercise (a flashcard): the prompt (front), the answer (back), and an
/// optional worked solution, all rendered HTML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseDef {
    pub id: String,
    pub rev: String,
    pub prompt: String,
    pub answer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solution: Option<String>,
}

/// One legend row: a symbol (rendered to MathML) and what it stands for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegendDef {
    /// The symbol, rendered to inline MathML.
    pub symbol: String,
    /// What the symbol means, as HTML-escaped text (safe to embed directly).
    pub meaning: String,
}

/// One reference: the rendered statement and an optional derivation. A deck
/// declares it, and the references its derivation links are what it rests on
/// (the `uses` edges).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceDef {
    pub title: String,
    pub rev: String,
    /// The bare equation, rendered to MathML.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reference: String,
    /// What each symbol in the equation stands for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub legend: Vec<LegendDef>,
    /// The explanation (statement prose).
    pub html: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derivation: Option<String>,
}

/// How one node relates to another in the content graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    /// A topic depends on a prerequisite topic.
    Requires,
    /// A reference rests on another reference: its derivation links it.
    Uses,
    /// A unit links a reference (the reference mark).
    Reference,
}

impl std::fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Requires => "requires",
            Self::Uses => "uses",
            Self::Reference => "reference",
        })
    }
}

/// A resolved graph edge: `[from, predicate, to]` with global path ids.
pub type Edge = (GlobalId, EdgeKind, GlobalId);

// ---------------------------------------------------------------------------
// Generated output: index.yaml (root catalogue of bundles)
// ---------------------------------------------------------------------------

/// The root catalogue the app loads first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    /// Every domain, flat. A domain declares the one language it is written in,
    /// so there is nothing to nest under (CONTENT-MODEL.md §9).
    pub domains: Vec<IndexDomain>,
}

/// A domain entry in the index, pointing at its bundle file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDomain {
    pub id: String,
    /// The BCP 47 tag the domain declares.
    pub lang: String,
    pub title: String,
    pub bundle: String,
}

// ---------------------------------------------------------------------------
// Per-user state sync
// ---------------------------------------------------------------------------

/// One row of a learner's state, as it is synced.
///
/// `kind` and `key` identify it, `value` is its opaque JSON payload, and
/// `updated_at` is the time of the last write, in ms since the epoch: the
/// later write wins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRow {
    pub kind: String,
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: f64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
