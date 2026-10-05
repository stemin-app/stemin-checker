//! What a file declares about itself: the front matter of every kind of node.
//!
//! Nothing here declares an id. An id is the filename stem, or the directory
//! name for a container, and depth decides the kind (CONTENT-MODEL.md §1). So
//! these types carry the title, the node's own metadata, and — for a container —
//! the `order` of its children.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What every node may say, whatever its kind.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Common {
    /// Shown to the learner. A container without one falls back to its id.
    #[serde(default)]
    pub title: Option<String>,
    /// Half-written: excluded from the `order` bijection, never built, never
    /// shown (CONTENT-MODEL.md §1.3).
    #[serde(default)]
    pub draft: bool,
    /// The children this container holds, in the order the learner meets them.
    /// Validated as a bijection with the directory, both ways.
    #[serde(default)]
    pub order: Option<Vec<String>>,
}

/// The repository's root `index.md`. An import reads this first, so it carries
/// everything the modal needs before it fetches anything else.
#[derive(Debug, Clone, Deserialize)]
pub struct Root {
    /// The format this repository targets, as semantic versioning. The app
    /// rejects a **major** it cannot read and accepts every minor and patch.
    pub format: String,
    #[serde(default)]
    pub author: Option<String>,
    /// One line, shown in the import modal before the learner commits.
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub order: Option<Vec<String>>,
}

/// A domain's `index.md`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Domain {
    #[serde(default)]
    pub title: Option<String>,
    /// The one language this domain is written in, as a BCP 47 tag. A
    /// declaration, never a selection (CONTENT-MODEL.md §9).
    pub lang: String,
    #[serde(default)]
    pub author: Option<String>,
    /// What this domain calls things, overriding the generic defaults.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub order: Option<Vec<String>>,
    #[serde(default)]
    pub draft: bool,
}

/// A section's `index.md`: the tier vocabulary, in order, and its topics.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Section {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tiers: Vec<String>,
    #[serde(default)]
    pub order: Option<Vec<String>>,
    #[serde(default)]
    pub draft: bool,
}

/// A topic's `index.md`: a chapter in the menu. It declares nothing but itself:
/// what it collects from its decks is derived.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Topic {
    #[serde(default)]
    pub title: Option<String>,
    /// One value from the section's `tiers`.
    #[serde(default)]
    pub tier: Option<String>,
    /// Topics this one rests on. A graph edge, which no directory can carry.
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub order: Option<Vec<String>>,
    #[serde(default)]
    pub draft: bool,
}

/// A deck's front matter. One line, in the common case.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Deck {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub draft: bool,
}

/// A folder of checkpoints: `checkpoints/` itself, or a folder under it. The
/// folders group the checkpoints as their author sees fit, for example one per
/// university and one per year.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CheckpointFolder {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub order: Option<Vec<String>>,
    #[serde(default)]
    pub draft: bool,
}

/// A checkpoint's front matter. Its instructions are the prose, and its
/// questions are `::: exercise` blocks of its own.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Checkpoint {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub draft: bool,
}

/// Every label a domain may override, with the **generic default** it falls back
/// to (CONTENT-MODEL.md §1.5).
///
/// The defaults are generic on purpose: a course that declares nothing reads
/// neutrally, and one that wants its own words says so once. A label changes
/// display only. It never touches an id, a route, a storage key, or progress.
pub const LABELS: [(&str, &str); 12] = [
    ("domain", "Domain"),
    ("section", "Section"),
    ("topic", "Topic"),
    ("deck", "Deck"),
    ("card", "Card"),
    ("exercise", "Exercise"),
    ("solution", "Solution"),
    ("reference", "Reference"),
    ("references", "References"),
    ("derivation", "Derivation"),
    ("checkpoint", "Checkpoint"),
    ("checkpoints", "Checkpoints"),
];

/// Every label name, as one line for an error message.
#[must_use]
pub fn label_names() -> String {
    LABELS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The front-matter keys each kind of file takes. A key outside its list is
/// an error, so a misspelt `requries:` does not vanish in silence.
#[must_use]
pub const fn keys(kind: crate::tree::Kind) -> &'static [&'static str] {
    use crate::tree::Kind;
    match kind {
        Kind::Root => &["format", "author", "description", "order"],
        Kind::Domain => &["title", "lang", "author", "labels", "order", "draft"],
        Kind::Section => &["title", "tiers", "order", "draft"],
        Kind::Topic => &["title", "tier", "requires", "order", "draft"],
        Kind::CheckpointFolder => &["title", "order", "draft"],
        Kind::Deck | Kind::Checkpoint => &["title", "draft"],
    }
}

/// What a domain calls one thing: its override, else the documented default,
/// else the key itself.
#[must_use]
pub fn label<'a>(overrides: &'a BTreeMap<String, String>, key: &'a str) -> &'a str {
    if let Some(own) = overrides.get(key) {
        return own;
    }
    LABELS
        .iter()
        .find(|(name, _)| *name == key)
        .map_or(key, |(_, default)| *default)
}

/// The format version this build reads. It rejects a **major** it does not know
/// and accepts every minor and patch, so the format can gain fields without
/// breaking a repository that does not use them.
pub const READS_MAJOR: u64 = 2;

/// One parsed semantic version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// Parse `major.minor.patch`.
    ///
    /// # Errors
    /// Returns what is wrong with the string, for the author to read.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut parts = text.trim().split('.');
        let mut next = |what: &str| -> Result<u64, String> {
            parts
                .next()
                .ok_or_else(|| format!("`{text}` has no {what}"))?
                .parse::<u64>()
                .map_err(|_| format!("`{text}` has a {what} that is not a number"))
        };
        let major = next("major")?;
        let minor = next("minor")?;
        let patch = next("patch")?;
        if parts.next().is_some() {
            return Err(format!("`{text}` has more than three parts"));
        }
        Ok(Self {
            major,
            minor,
            patch,
        })
    }

    /// Whether this build can read a repository at this version.
    #[must_use]
    pub const fn readable(self) -> bool {
        self.major == READS_MAJOR
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{LABELS, Version, label};

    #[test]
    fn a_label_falls_back_to_its_documented_default() {
        let none = BTreeMap::new();
        assert_eq!(label(&none, "reference"), "Reference");
        assert_eq!(label(&none, "references"), "References");
        assert_eq!(label(&none, "derivation"), "Derivation");
        assert_eq!(label(&none, "checkpoint"), "Checkpoint");
    }

    #[test]
    fn a_domain_overrides_only_what_it_names() {
        let mut own = BTreeMap::new();
        own.insert("reference".to_owned(), "Formula".to_owned());
        own.insert("derivation".to_owned(), "Proof".to_owned());
        assert_eq!(label(&own, "reference"), "Formula");
        assert_eq!(label(&own, "derivation"), "Proof");
        // Untouched, so still the generic default.
        assert_eq!(label(&own, "checkpoint"), "Checkpoint");
    }

    /// Every default is documented. A label with no default would read as its
    /// own key in the interface, which is a bug the table above prevents.
    #[test]
    fn every_label_has_a_default() {
        assert_eq!(LABELS.len(), 12);
        for (key, default) in LABELS {
            assert!(!default.is_empty(), "{key} has no default");
            // A plural key reads plural. `references` once defaulted to
            // "Reference", so a domain that renamed the singular and not the
            // plural got a heading in the wrong number.
            assert_eq!(
                key.ends_with('s'),
                default.ends_with('s'),
                "{key} and {default} disagree on number"
            );
        }
    }

    #[test]
    fn a_version_parses_and_rejects_the_wrong_major() {
        assert!(Version::parse("2.0.0").expect("parses").readable());
        assert!(Version::parse("2.7.3").expect("parses").readable());
        // Format 1 kept references in their own folder and checkpoints in a
        // topic: it does not parse as 2, and nothing converts it.
        assert!(!Version::parse("1.0.0").expect("parses").readable());
        assert!(!Version::parse("3.0.0").expect("parses").readable());
    }

    #[test]
    fn a_malformed_version_says_what_is_wrong() {
        assert!(Version::parse("1").is_err());
        assert!(Version::parse("1.0").is_err());
        assert!(Version::parse("1.0.0.0").is_err());
        assert!(Version::parse("one.0.0").is_err());
    }
}
