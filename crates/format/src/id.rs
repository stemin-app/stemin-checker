//! Content ids: the global path grammar, and the hash behind a domain id.
//!
//! An id is a filename stem, or a directory name for a container
//! (CONTENT-MODEL.md §1). Nothing in a repository declares one.

use serde::{Deserialize, Serialize};

/// How many hex characters of the hash a domain id takes.
///
/// A learner holds a handful of domains, and the import refuses a collision
/// outright, so 32 bits is ample. Eight characters also keep a content URL
/// short enough to read.
const DOMAIN_ID_LEN: usize = 8;

/// One content repository's URL, in the single form a domain id derives from.
///
/// The same repository can be written many ways: with or without a scheme, with
/// or without a trailing `.git`, in any letter case, with a browser's query
/// string still attached. Each spelling would otherwise hash to a different
/// domain, and the learner's progress would split between them. So every
/// spelling folds to `host/owner/name` first.
///
/// The whole string lowercases, not the host alone: GitHub, GitLab and Forgejo
/// all treat an owner and a repository name case-insensitively, so
/// `github.com/Alice/physics` and `github.com/alice/physics` are
/// one repository and must be one domain.
///
/// ```
/// use stemin_format::id::canonical_repo_url;
/// assert_eq!(canonical_repo_url("https://GitHub.com/Me/Course.git/"), "github.com/me/course");
/// ```
#[must_use]
pub fn canonical_repo_url(url: &str) -> String {
    let mut rest = url.trim();
    // Drop the scheme. An `scp`-like address (`git@host:owner/name`) carries no
    // scheme, and the `user@` and the `:` are handled below.
    for scheme in ["https://", "http://", "git://", "ssh://"] {
        if let Some(tail) = rest.strip_prefix(scheme) {
            rest = tail;
            break;
        }
    }
    // Drop a query string or a fragment, which a pasted browser URL carries.
    rest = rest.split(['?', '#']).next().unwrap_or(rest);
    // Drop credentials, so `git@github.com:me/course` folds with the rest.
    if let Some((_, tail)) = rest.split_once('@') {
        rest = tail;
    }
    let lower = rest.to_lowercase();
    // An `scp`-like address separates the host from the path with a colon, and
    // so does a port. Only the first is a path separator: folding a port would
    // turn `git.example.com:3000/me/course` into a repository called
    // `me/course` owned by `3000`, and lose the port the host answers on.
    let mut out = match lower.split_once(':') {
        Some((host, tail)) if !starts_with_port(tail) => format!("{host}/{tail}"),
        _ => lower,
    };
    // Drop the trailing `/` and the trailing `.git`, in that order, because a
    // URL can carry both.
    out = out.trim_end_matches('/').to_owned();
    if let Some(head) = out.strip_suffix(".git") {
        out = head.trim_end_matches('/').to_owned();
    }
    drop_browse_path(&out)
}

/// Whether what follows a colon is a port, rather than an `scp`-style path.
fn starts_with_port(tail: &str) -> bool {
    let digits = tail.split('/').next().unwrap_or(tail);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// The segments a forge puts after a repository when it browses inside it.
///
/// A learner copies the URL from the address bar, so the paste often carries
/// `/tree/main` (GitHub), `/-/tree/main` (GitLab) or `/src/section/main`
/// (Forgejo). Without this the paste would hash to its own domain, the progress
/// would key to it, and nothing would ever say why.
const BROWSE_MARKERS: [&str; 11] = [
    "-", "tree", "blob", "src", "raw", "commit", "commits", "releases", "pull", "issues", "wiki",
];

/// Cut a canonical URL back to the repository, dropping any browse path.
///
/// The scan starts at the fourth segment, so a repository that is itself called
/// `tree` survives, and it never counts segments, so a GitLab subgroup
/// (`gitlab.com/group/subgroup/project`) survives too.
fn drop_browse_path(url: &str) -> String {
    let segments: Vec<&str> = url.split('/').collect();
    let cut = segments
        .iter()
        .enumerate()
        .skip(3)
        .find(|(_, segment)| BROWSE_MARKERS.contains(*segment))
        .map_or(segments.len(), |(at, _)| at);
    segments
        .get(..cut)
        .unwrap_or(&segments)
        .join("/")
        .trim_end_matches('/')
        .to_owned()
}

/// The domain id: a hash over a repository's canonical URL and one domain's
/// directory name.
///
/// This is the first segment of every content id and of every URL in the app.
/// It is a hash, not the name the content declares, for three reasons. One
/// repository holds several domains, and each needs its own id. Two
/// repositories may both call a domain `phy`, and they must never share a
/// progress row. And every device derives the same id from the same pair on its
/// own, so nothing about identity needs to sync.
///
/// The **ref is not in it**: a section and a tag of one repository are one domain
/// at two versions, and the diff carries the progress between them.
///
/// `domain` is the domain's **directory name** at the repository root. Nothing
/// declares it: an id is a filename stem, or a directory name for a container
/// (CONTENT-MODEL.md §1).
///
/// It is taken **verbatim**, unlike the URL. A host treats a repository name
/// case-insensitively, so the URL folds; a git tree does not, so `Phy/` and
/// `phy/` are two directories and stay two domains.
///
/// Renaming the directory therefore moves the domain's id, and every progress
/// row under it. The refresh covers that: every file under the domain moves
/// with its content hash unchanged, which is the rename the diff already
/// detects for a deck, one level up. **Reordering costs nothing**, because
/// `order:` names ids and never touches a path.
///
/// ```
/// use stemin_format::id::domain_id;
/// assert_eq!(
///     domain_id("https://github.com/me/course.git", "phy"),
///     domain_id("github.com/Me/Course", "phy"),
/// );
/// assert_ne!(
///     domain_id("github.com/me/course", "phy"),
///     domain_id("github.com/me/course", "math"),
/// );
/// ```
#[must_use]
pub fn domain_id(repo_url: &str, domain: &str) -> String {
    let seed = format!("{}/{domain}", canonical_repo_url(repo_url));
    blake3::hash(seed.as_bytes()).to_hex()[..DOMAIN_ID_LEN].to_owned()
}

/// The id of a repository: a hash over its canonical URL alone.
///
/// A domain id hashes the URL **and** the domain's directory name, because one
/// repository holds several domains. This one names the repository itself: the
/// row on the server, the tree map on the device, and the menu entry the
/// learner refreshes or deletes.
///
/// ```
/// use stemin_format::id::{domain_id, repo_id};
/// assert_eq!(repo_id("https://github.com/me/course.git"), repo_id("github.com/Me/Course"));
/// assert_ne!(repo_id("github.com/me/course"), domain_id("github.com/me/course", "phy"));
/// ```
#[must_use]
pub fn repo_id(url: &str) -> String {
    let canonical = canonical_repo_url(url);
    blake3::hash(canonical.as_bytes()).to_hex()[..DOMAIN_ID_LEN].to_owned()
}

/// A global content id (e.g. `math/algebra/linears/intro#e1`).
///
/// A `/`-separated path optionally ending in a `#ref` suffix. The path grammar
/// lives here so callers build and decompose ids through it, not by hand.
///
/// The first segment is the **domain id**, which [`domain_id`] derives from the
/// repository URL. The name the content declares for itself is a title, never
/// an id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GlobalId(String);

impl GlobalId {
    /// Wrap an existing id.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The domain (the first path segment).
    #[must_use]
    pub fn domain(&self) -> &str {
        self.0.split('/').next().unwrap_or(&self.0)
    }

    /// The parent path (everything before the last `/` segment), if any.
    #[must_use]
    pub fn parent(&self) -> Option<&str> {
        self.0.rsplit_once('/').map(|(head, _)| head)
    }

    /// Extend the path with a child segment: `self/segment`.
    #[must_use]
    pub fn child(&self, segment: &str) -> Self {
        Self(format!("{}/{segment}", self.0))
    }

    /// Form a unit reference within this id: `self#id`.
    #[must_use]
    pub fn reference(&self, id: &str) -> Self {
        Self(format!("{}#{id}", self.0))
    }
}

impl std::fmt::Display for GlobalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::borrow::Borrow<str> for GlobalId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl From<&str> for GlobalId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<GlobalId> for String {
    fn from(id: GlobalId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use super::{DOMAIN_ID_LEN, canonical_repo_url, domain_id};

    /// The id of `phy` in `github.com/alice/stem`, computed once and
    /// pinned. See [`the_id_is_stable`].
    const PINNED_PHY_ID: &str = "9c8f9b9e";
    /// The id of `math` in the same repository, to prove the seed carries both
    /// halves and not the URL alone.
    const PINNED_MATH_ID: &str = "0d0d4df6";

    /// Every spelling of one repository folds to one canonical form.
    #[test]
    fn canonical_form_folds_every_spelling() {
        let want = "github.com/alice/physics";
        for spelling in [
            "https://github.com/alice/physics",
            "http://github.com/alice/physics",
            "github.com/alice/physics",
            "https://github.com/alice/physics/",
            "https://github.com/alice/physics.git",
            "https://github.com/alice/physics.git/",
            "https://GitHub.com/Alice/Physics",
            "  https://github.com/alice/physics  ",
            "https://github.com/alice/physics?tab=readme-ov-file",
            "https://github.com/alice/physics#readme",
            "git://github.com/alice/physics.git",
            "ssh://git@github.com/alice/physics.git",
            "git@github.com:alice/physics.git",
        ] {
            assert_eq!(canonical_repo_url(spelling), want, "spelling: {spelling}");
        }
    }

    /// One domain keeps one id, however the learner spells the repository.
    #[test]
    fn one_domain_keeps_one_id() {
        let id = domain_id("https://github.com/alice/stem", "phy");
        for spelling in [
            "github.com/Alice/Stem",
            "git@github.com:alice/stem.git",
            "https://github.com/alice/stem.git/",
            "https://github.com/alice/stem/tree/main",
        ] {
            assert_eq!(domain_id(spelling, "phy"), id, "spelling: {spelling}");
        }
    }

    /// One repository holds several domains, and each gets its own id.
    #[test]
    fn domains_in_one_repository_get_different_ids() {
        let url = "github.com/alice/stem";
        let phy = domain_id(url, "phy");
        let math = domain_id(url, "math");
        let chem = domain_id(url, "chem");
        assert_ne!(phy, math);
        assert_ne!(phy, chem);
        assert_ne!(math, chem);
    }

    /// The directory name is taken verbatim: a git tree is case-sensitive, so
    /// two differently-cased directories stay two domains, unlike the URL.
    #[test]
    fn the_directory_name_is_case_sensitive() {
        let url = "github.com/alice/stem";
        assert_ne!(domain_id(url, "phy"), domain_id(url, "Phy"));
    }

    /// Two repositories never share an id, even when they name a domain alike.
    /// A mirror on another host is its own domain until the learner edits the
    /// URL of the one they already hold.
    #[test]
    fn different_repositories_get_different_ids() {
        let mine = domain_id("github.com/alice/stem", "phy");
        let theirs = domain_id("github.com/someone/stem", "phy");
        let mirror = domain_id("git.example.com/alice/stem", "phy");
        assert_ne!(mine, theirs);
        assert_ne!(mine, mirror);
    }

    /// A URL copied from a forge's address bar names the repository, not the
    /// page. The ref in it is dropped, because a section and a tag are one domain
    /// at two versions and the diff carries the progress between them.
    #[test]
    fn a_browse_url_names_the_repository() {
        let want = "github.com/alice/physics";
        for browsing in [
            "https://github.com/alice/physics/tree/main",
            "https://github.com/alice/physics/tree/v2.1/en/phy",
            "https://github.com/alice/physics/blob/main/stemin.yaml",
            "https://github.com/alice/physics/commits/main",
            "https://github.com/alice/physics/releases",
        ] {
            assert_eq!(canonical_repo_url(browsing), want, "browsing: {browsing}");
        }
        assert_eq!(
            canonical_repo_url("https://codeberg.org/me/course/src/section/main"),
            "codeberg.org/me/course",
        );
        assert_eq!(
            canonical_repo_url("https://gitlab.com/group/sub/course/-/tree/main"),
            "gitlab.com/group/sub/course",
        );
    }

    /// A GitLab subgroup is part of the repository's address, so it survives.
    /// The browse scan looks for a marker, and never counts segments.
    #[test]
    fn a_subgroup_survives() {
        assert_eq!(
            canonical_repo_url("https://gitlab.com/group/sub/deeper/course.git"),
            "gitlab.com/group/sub/deeper/course",
        );
    }

    /// A repository may be named after a marker word. The scan starts past the
    /// repository name, so such a name is never mistaken for a browse path.
    #[test]
    fn a_repository_named_after_a_marker_survives() {
        assert_eq!(
            canonical_repo_url("https://github.com/me/tree"),
            "github.com/me/tree",
        );
        assert_eq!(
            canonical_repo_url("https://github.com/me/blob.git"),
            "github.com/me/blob",
        );
    }

    /// An id is a fixed-width, lowercase hex string, so it is safe as the first
    /// segment of a URL and of every store key.
    #[test]
    fn an_id_is_url_safe_and_fixed_width() {
        let id = domain_id("github.com/alice/stem", "phy");
        assert_eq!(id.len(), DOMAIN_ID_LEN);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
        );
    }

    /// The id is stable across builds and devices. Both values are a contract:
    /// changing either one moves every learner's progress to a new domain, and
    /// the progress does not follow. The hash is pinned beside the string it
    /// covers, so a change to the canonical form cannot pass unnoticed.
    #[test]
    fn the_id_is_stable() {
        let url = "https://github.com/Alice/stem.git";
        assert_eq!(canonical_repo_url(url), "github.com/alice/stem");
        assert_eq!(domain_id(url, "phy"), PINNED_PHY_ID);
        assert_eq!(domain_id(url, "math"), PINNED_MATH_ID);
    }
}
