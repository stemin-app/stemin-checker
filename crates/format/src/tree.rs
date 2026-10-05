//! Walk a content repository into a tree of nodes.
//!
//! Containment is the directory, an id is the filename stem, and depth decides
//! the kind (CONTENT-MODEL.md §1). So this walk reads a set of files and needs
//! no declaration of structure at all. `order` only sorts what it finds, and
//! [`crate::check`] holds the two to a bijection.
//!
//! **The walk takes a map, not a directory.** The browser holds a repository as
//! paths and bytes it fetched from a forge, and the author holds one on disk.
//! One walk over [`Files`] serves both, so the checker an author runs in CI and
//! the compiler that runs in the learner's browser can never read one
//! repository two ways.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::block::Document;
use crate::error::{Fault, FaultKind};

/// The reserved names. Everything else in a repository is content.
pub const INDEX: &str = "index.md";
/// The directory that holds a domain's checkpoints, beside its sections.
pub const CHECKPOINTS_DIR: &str = "checkpoints";
/// The directory that holds inlined assets, at any level.
pub const ASSETS_DIR: &str = "assets";

/// Assets the format carries, and the cap on any one of them.
pub const ASSET_TYPES: [&str; 5] = ["png", "jpg", "jpeg", "webp", "svg"];
/// Bytes. Past this the author optimises the image before committing it.
pub const ASSET_CAP: u64 = 256 * 1024;
/// Bytes in one `.md` file. A deck this long is many decks, and the cap keeps
/// a crafted file from taking the parser's time and memory.
pub const TEXT_CAP: u64 = 512 * 1024;
/// How many path segments a file may have below the repository root.
///
/// The count includes the file's own name. The tree needs five
/// (`domain/section/topic/assets/x.png`), and checkpoint folders nest. Past
/// this nothing is read, so no walk over the tree recurses without a bound.
pub const MAX_DEPTH: usize = 16;

/// One file of a repository.
#[derive(Debug, Clone, Default)]
pub struct Blob {
    /// The bytes. Empty for a file too big to be worth holding: the size alone
    /// is enough to refuse it, and nothing will ever inline it.
    pub bytes: Vec<u8>,
    /// What the file weighs, which is not always `bytes.len()`: a host's tree
    /// API gives a size before a byte is fetched.
    pub size: u64,
}

impl Blob {
    /// A file held in full.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            size: bytes.len().try_into().unwrap_or(u64::MAX),
            bytes,
        }
    }

    /// The file as text, where it is valid UTF-8.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }
}

/// A repository as paths and bytes. Every key is a `/`-separated path.
pub type Files = BTreeMap<String, Blob>;

/// What a node is. Depth decides it; nothing declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Root,
    Domain,
    Section,
    Topic,
    Deck,
    /// `checkpoints/`, or a folder under it.
    CheckpointFolder,
    /// One checkpoint: a file under `checkpoints/`, at any depth.
    Checkpoint,
}

impl Kind {
    /// Whether a node of this kind is a **directory** with its own `index.md`.
    /// Everything else is a single file. This is what stops a stray `README.md`
    /// at the root being read as a domain, and what stops a directory and a
    /// file of the same stem from colliding.
    #[must_use]
    pub const fn is_container(self) -> bool {
        matches!(
            self,
            Self::Root | Self::Domain | Self::Section | Self::Topic | Self::CheckpointFolder
        )
    }

    /// The word for this kind, for a message.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Root => "repository",
            Self::Domain => "domain",
            Self::Section => "section",
            Self::Topic => "topic",
            Self::Deck => "deck",
            Self::CheckpointFolder => "checkpoint folder",
            Self::Checkpoint => "checkpoint",
        }
    }

    /// The kind a container's children take. A checkpoint folder holds two
    /// kinds, folders and checkpoints, and the walk tells them apart by whether
    /// each is a directory or a file.
    #[must_use]
    pub const fn child(self) -> Option<Self> {
        match self {
            Self::Root => Some(Self::Domain),
            Self::Domain => Some(Self::Section),
            Self::Section => Some(Self::Topic),
            Self::Topic => Some(Self::Deck),
            Self::CheckpointFolder => Some(Self::Checkpoint),
            Self::Deck | Self::Checkpoint => None,
        }
    }
}

/// One node: where it came from, what it is, and what it holds.
#[derive(Debug, Clone)]
pub struct Node {
    /// The filename stem, or the directory name for a container. Empty at the
    /// root, which is the repository itself.
    pub id: String,
    pub kind: Kind,
    /// The file this node was read from, as a key of [`Files`].
    pub path: String,
    pub doc: Document,
    /// Children, already in `order` where the container declared one.
    pub children: Vec<Self>,
    /// Assets found beside this node, as keys of [`Files`].
    pub assets: Vec<String>,
    /// Ids `order` named that no file carries.
    pub missing: Vec<String>,
    /// Ids present here that `order` did not name.
    pub unlisted: Vec<String>,
}

impl Node {
    /// Every node under this one, this one included, depth first.
    #[must_use]
    pub fn walk(&self) -> Vec<&Self> {
        // An explicit stack, not recursion: the order is the same, and no
        // depth of folders can overflow the stack.
        let mut out = Vec::new();
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            out.push(node);
            stack.extend(node.children.iter().rev());
        }
        out
    }

    /// The directory this node's file sits in, as a key prefix. A reference to
    /// an asset resolves against it, exactly as in any markdown tool.
    #[must_use]
    pub fn dir(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(head, _)| head)
    }
}

/// Read a repository held as files.
///
/// `root` is the directory whose `index.md` is the repository root: the empty
/// string for a repository fetched from a forge, or the path the author gave on
/// the command line.
///
/// Returns **both** what it managed to parse and every fault it met, never one
/// or the other. A single bad file must not hide the rest of the repository's
/// faults, because an author fixing CI wants the whole list in one run.
#[must_use]
pub fn read_map(root: &str, files: &Files) -> (Option<Node>, Vec<Fault>) {
    let repo = Repo::index(root, files);
    let mut faults: Vec<Fault> = repo
        .too_deep
        .iter()
        .map(|path| Fault::file(path, FaultKind::PathTooDeep(MAX_DEPTH)))
        .collect();
    let index = Repo::join(root, INDEX);
    if !repo.files.contains_key(&index) {
        return (None, vec![Fault::file(index, FaultKind::NoRoot)]);
    }
    let node = repo.container(root, String::new(), Kind::Root, &mut faults);
    (node, faults)
}

/// The folder of every domain, from the paths of a repository alone.
///
/// A domain is a container at depth one, and a container holds an `index.md`
/// (CONTENT-MODEL.md §1). So this needs no file contents: the tree listing of a
/// host is enough. A delete on a device that never compiled the repository
/// uses it to name the domains whose progress it must clear.
///
/// It can name a folder that [`read_map`] would refuse. That costs nothing: a
/// prefix that no progress row carries clears nothing.
#[must_use]
pub fn domain_dirs<'a>(paths: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut dirs: Vec<String> = paths
        .into_iter()
        .filter_map(|path| path.split_once('/'))
        .filter(|(_, rest)| *rest == INDEX)
        .map(|(dir, _)| dir.to_owned())
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}

/// Read a repository from disk.
///
/// # Errors
/// Reports an unreadable directory or file as a fault, like any other.
#[cfg(feature = "fs")]
#[must_use]
pub fn read(root: &std::path::Path) -> (Option<Node>, Vec<Fault>) {
    let mut files = Files::new();
    let mut found = Vec::new();
    let root_key = root.to_string_lossy().trim_end_matches('/').to_owned();
    gather_from_disk(root, &root_key, 0, &mut files, &mut found);
    let (node, faults) = read_map(&root_key, &files);
    found.extend(faults);
    (node, found)
}

/// Read every file under a directory into the map the walk takes, keyed from
/// the directory itself, as the browser holds a fetched repository. Walk it
/// with `read_map("", &files)`.
///
/// It also returns what the disk itself got wrong: a symbolic link, a special
/// file, or a file that would not open. A forge never serves these, so the
/// walk never sees them, and the caller reports them beside its faults.
#[cfg(feature = "fs")]
#[must_use]
pub fn files(root: &std::path::Path) -> (Files, Vec<Fault>) {
    let mut files = Files::new();
    let mut faults = Vec::new();
    gather_from_disk(root, "", 0, &mut files, &mut faults);
    (files, faults)
}

/// Read every file under a directory into the map the walk takes.
///
/// It never follows a symbolic link: a link can point out of the repository,
/// at a device, or at its own directory. It reads no file past its cap (the
/// text cap for a `.md` file, the asset cap for any other), so a file past it
/// is recorded by its size alone, and the walk refuses it. It goes no deeper
/// than [`MAX_DEPTH`]: a name at that depth is recorded with no bytes, and the
/// walk refuses it.
#[cfg(feature = "fs")]
fn gather_from_disk(
    dir: &std::path::Path,
    prefix: &str,
    depth: usize,
    files: &mut Files,
    faults: &mut Vec<Fault>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let depth = depth.saturating_add(1);
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let key = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        // The type of the entry itself. Unlike `Path::is_dir`, this does not
        // follow a link.
        let Ok(kind) = entry.file_type() else {
            faults.push(Fault::file(
                &key,
                FaultKind::Io("its type will not read".to_owned()),
            ));
            continue;
        };
        if kind.is_symlink() {
            faults.push(Fault::file(&key, FaultKind::Symlink(name.to_owned())));
        } else if depth > MAX_DEPTH {
            files.insert(key, Blob::default());
        } else if kind.is_dir() {
            gather_from_disk(&path, &key, depth, files, faults);
        } else if kind.is_file() {
            let text = name.to_ascii_lowercase().ends_with(".md");
            let cap = if text { TEXT_CAP } else { ASSET_CAP };
            match read_capped(&path, cap) {
                Ok(blob) => {
                    files.insert(key, blob);
                }
                Err(err) => faults.push(Fault::file(&key, FaultKind::Io(err.to_string()))),
            }
        } else {
            faults.push(Fault::file(&key, FaultKind::NotAFile(name.to_owned())));
        }
    }
}

/// Read one file, but never more than `cap` bytes of it. A file past the cap
/// keeps its size and no bytes, which is all a fault needs.
#[cfg(feature = "fs")]
fn read_capped(path: &std::path::Path, cap: u64) -> std::io::Result<Blob> {
    use std::io::Read as _;
    let file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size > cap {
        return Ok(Blob {
            bytes: Vec::new(),
            size,
        });
    }
    // The size on disk can change, or lie, so the read holds to the cap too.
    let mut bytes = Vec::new();
    file.take(cap.saturating_add(1)).read_to_end(&mut bytes)?;
    let read = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if read > cap {
        return Ok(Blob {
            bytes: Vec::new(),
            size: read,
        });
    }
    Ok(Blob::new(bytes))
}

/// The repository, indexed so the walk can ask what sits in a directory.
struct Repo<'a> {
    files: &'a Files,
    /// Every directory, with the names of its immediate children.
    dirs: BTreeMap<String, BTreeSet<String>>,
    /// Every directory, so a name can be told from a file.
    is_dir: BTreeSet<String>,
    /// Every file that sits past [`MAX_DEPTH`], which the walk never reaches.
    too_deep: Vec<String>,
}

impl<'a> Repo<'a> {
    /// Index the map: which names sit directly in which directory.
    fn index(root: &str, files: &'a Files) -> Self {
        let mut dirs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut is_dir: BTreeSet<String> = BTreeSet::new();
        let mut too_deep: Vec<String> = Vec::new();
        let under = if root.is_empty() {
            String::new()
        } else {
            format!("{root}/")
        };
        for path in files.keys() {
            if !under.is_empty() && !path.starts_with(&under) {
                continue;
            }
            let mut dir = root.to_owned();
            let rest = path.get(under.len()..).unwrap_or_default();
            if rest.split('/').count() > MAX_DEPTH {
                too_deep.push(path.clone());
                continue;
            }
            let mut parts = rest.split('/').peekable();
            while let Some(part) = parts.next() {
                dirs.entry(dir.clone()).or_default().insert(part.to_owned());
                let child = if dir.is_empty() {
                    part.to_owned()
                } else {
                    format!("{dir}/{part}")
                };
                if parts.peek().is_some() {
                    is_dir.insert(child.clone());
                }
                dir = child;
            }
        }
        Self {
            files,
            dirs,
            is_dir,
            too_deep,
        }
    }

    /// A path under a directory.
    fn join(dir: &str, name: &str) -> String {
        if dir.is_empty() {
            name.to_owned()
        } else {
            format!("{dir}/{name}")
        }
    }

    /// Read one container: its own `index.md`, then its children in `order`.
    fn container(
        &self,
        dir: &str,
        id: String,
        kind: Kind,
        faults: &mut Vec<Fault>,
    ) -> Option<Node> {
        let path = Self::join(dir, INDEX);
        let doc = self.doc(&path, faults)?;
        let order = declared_order(&doc, &path, faults);

        let mut found: Vec<(String, Entry)> = Vec::new();
        let mut assets = Vec::new();
        let mut drafts = Vec::new();
        self.collect(dir, kind, &mut found, &mut assets, &mut drafts, faults);

        let (children, missing, unlisted) =
            self.arrange(&path, found, &drafts, order.as_deref(), kind, faults);
        Some(Node {
            id,
            kind,
            path,
            doc,
            children,
            assets,
            missing,
            unlisted,
        })
    }

    /// Gather a directory's children and its assets, skipping the reserved
    /// names.
    fn collect(
        &self,
        dir: &str,
        kind: Kind,
        found: &mut Vec<(String, Entry)>,
        assets: &mut Vec<String>,
        drafts: &mut Vec<String>,
        faults: &mut Vec<Fault>,
    ) {
        let child_kind = kind.child().unwrap_or(Kind::Deck);
        // A checkpoint folder holds folders and files alike; the domain holds
        // one folder beside its sections, `checkpoints/`.
        let holds_folders = kind == Kind::CheckpointFolder;
        let holds_files = kind == Kind::CheckpointFolder || !child_kind.is_container();
        let Some(names) = self.dirs.get(dir) else {
            return;
        };
        for name in names {
            let path = Self::join(dir, name);
            if self.is_dir.contains(&path) {
                if name == ASSETS_DIR {
                    self.gather_assets(&path, assets, faults);
                } else if name.starts_with('.') {
                    // A dot directory is never content.
                } else if holds_folders
                    || child_kind.is_container()
                    || (kind == Kind::Domain && name == CHECKPOINTS_DIR)
                {
                    // A directory is a child only where a child is a container.
                    // `checkpoints/` is one like any other; only its kind
                    // differs, which `arrange` decides.
                    if self.is_draft(&Self::join(&path, INDEX)) {
                        drafts.push(name.clone());
                    } else {
                        found.push((name.clone(), Entry::Container(path)));
                    }
                } else {
                    // A directory where only files belong can never be reached,
                    // so it would be dropped in silence. An author who nested
                    // one level too deep gets told.
                    faults.push(Fault::file(&path, FaultKind::Unreachable(name.clone())));
                }
                continue;
            }
            if name == INDEX {
                // The container's own file, already read.
                continue;
            }
            let (stem, ext) = split_name(name);
            match ext.as_deref() {
                // A `.md` file is a child only where a child is a file. So a
                // `README.md` beside the domains is not a domain, and a stray
                // note beside the sections is not a section.
                Some("md") if holds_files => {
                    if self.is_draft(&path) {
                        drafts.push(stem.to_owned());
                    } else {
                        found.push((stem.to_owned(), Entry::Leaf(path)));
                    }
                }
                Some("md") => {}
                Some(ext) if ASSET_TYPES.contains(&ext) => {
                    faults.push(Fault::file(&path, FaultKind::AssetMisplaced(name.clone())));
                }
                _ => {}
            }
        }
    }

    /// Check every file in an `assets/` directory, and keep it for inlining.
    fn gather_assets(&self, dir: &str, assets: &mut Vec<String>, faults: &mut Vec<Fault>) {
        let Some(names) = self.dirs.get(dir) else {
            return;
        };
        for name in names {
            let path = Self::join(dir, name);
            if self.is_dir.contains(&path) {
                self.gather_assets(&path, assets, faults);
                continue;
            }
            let (_, ext) = split_name(name);
            let ext = ext.unwrap_or_default();
            if !ASSET_TYPES.contains(&ext.as_str()) {
                faults.push(Fault::file(&path, FaultKind::AssetType(name.clone())));
                continue;
            }
            let size = self.files.get(&path).map_or(0, |blob| blob.size);
            if size > ASSET_CAP {
                faults.push(Fault::file(
                    &path,
                    FaultKind::AssetTooBig {
                        path: name.clone(),
                        size,
                        cap: ASSET_CAP,
                    },
                ));
            }
            assets.push(path);
        }
    }

    /// Whether a node marks itself half-written, and so sits outside the
    /// `order` bijection (CONTENT-MODEL.md §1.3).
    ///
    /// This runs on a container's `index.md` too, not only on a leaf: a section
    /// or a topic can be unfinished exactly as a deck can, and an author who
    /// marks one a draft must not ship it.
    ///
    /// A file that will not even parse is not a draft: the parse fault is
    /// reported in its own right.
    fn is_draft(&self, path: &str) -> bool {
        #[derive(serde::Deserialize)]
        struct Draft {
            #[serde(default)]
            draft: bool,
        }
        self.files
            .get(path)
            .filter(|blob| text_size(blob) <= TEXT_CAP)
            .and_then(Blob::text)
            .and_then(|src| crate::block::parse(src).ok())
            .and_then(|doc| serde_yaml::from_str::<Draft>(&doc.front).ok())
            .is_some_and(|front| front.draft)
    }

    /// Put the children in `order`, and record both halves of a broken
    /// bijection.
    fn arrange(
        &self,
        at: &str,
        found: Vec<(String, Entry)>,
        drafts: &[String],
        order: Option<&[String]>,
        kind: Kind,
        faults: &mut Vec<Fault>,
    ) -> (Vec<Node>, Vec<String>, Vec<String>) {
        let child_kind = kind.child().unwrap_or(Kind::Deck);
        let mut by_id: BTreeMap<String, Entry> = found.into_iter().collect();
        let drafts: HashSet<&String> = drafts.iter().collect();

        // A domain's `checkpoints/` is not a section, so its `order` does not
        // list it: it stands beside the sections, and Practice shows it.
        let checkpoints = if kind == Kind::Domain
            && let Some(Entry::Container(dir)) = by_id.remove(CHECKPOINTS_DIR)
        {
            self.container(
                &dir,
                CHECKPOINTS_DIR.to_owned(),
                Kind::CheckpointFolder,
                faults,
            )
        } else {
            None
        };

        // No `order`: alphabetical, which `BTreeMap` already gives.
        let sequence: Vec<String> =
            order.map_or_else(|| by_id.keys().cloned().collect(), <[String]>::to_vec);

        let mut children = Vec::new();
        let mut missing = Vec::new();
        let mut taken: HashSet<&String> = HashSet::new();
        for id in &sequence {
            if !taken.insert(id) {
                faults.push(Fault::file(at, FaultKind::OrderRepeats(id.clone())));
                continue;
            }
            match by_id.remove(id) {
                Some(Entry::Container(dir)) => {
                    // A folder under a checkpoint folder is a folder too.
                    let this = if kind == Kind::CheckpointFolder {
                        Kind::CheckpointFolder
                    } else {
                        child_kind
                    };
                    if let Some(node) = self.container(&dir, id.clone(), this, faults) {
                        children.push(node);
                    }
                }
                Some(Entry::Leaf(path)) => {
                    let this = child_kind;
                    if let Some(doc) = self.doc(&path, faults) {
                        children.push(Node {
                            id: id.clone(),
                            kind: this,
                            path,
                            doc,
                            children: Vec::new(),
                            assets: Vec::new(),
                            missing: Vec::new(),
                            unlisted: Vec::new(),
                        });
                    }
                }
                // Naming a draft is its own mistake: the file is plainly there,
                // so "no file carries that id" would read as a lie.
                None if drafts.contains(id) => {
                    faults.push(Fault::file(at, FaultKind::OrderNamesDraft(id.clone())));
                }
                None => missing.push(id.clone()),
            }
        }
        // Whatever `order` did not name is left over.
        let unlisted = by_id.into_keys().collect();
        // The checkpoints come last, after every section.
        children.extend(checkpoints);
        (children, missing, unlisted)
    }

    /// Read and parse one file, recording a fault rather than stopping.
    fn doc(&self, path: &str, faults: &mut Vec<Fault>) -> Option<Document> {
        let Some(blob) = self.files.get(path) else {
            faults.push(Fault::file(path, FaultKind::Io("no such file".to_owned())));
            return None;
        };
        let size = text_size(blob);
        if size > TEXT_CAP {
            faults.push(Fault::file(
                path,
                FaultKind::TextTooBig {
                    size,
                    cap: TEXT_CAP,
                },
            ));
            return None;
        }
        let Some(src) = blob.text() else {
            faults.push(Fault::file(
                path,
                FaultKind::Io("this file is not UTF-8 text".to_owned()),
            ));
            return None;
        };
        match crate::block::parse(src) {
            Ok(doc) => Some(doc),
            Err((line, kind)) => {
                faults.push(Fault::line(path, line, kind));
                None
            }
        }
    }
}

/// What a text file weighs: its declared size, or its bytes where they say
/// more. A host can declare a size of its own, and the bytes are what parse.
fn text_size(blob: &Blob) -> u64 {
    blob.size
        .max(u64::try_from(blob.bytes.len()).unwrap_or(u64::MAX))
}

/// What sits in a container's directory, before `order` is applied.
enum Entry {
    Container(String),
    Leaf(String),
}

/// A filename split into its stem and its lowercased extension.
///
/// Lowercased because a `logo.PNG` outside an `assets/` directory must be
/// reported, not dropped in silence.
fn split_name(name: &str) -> (&str, Option<String>) {
    name.rsplit_once('.').map_or((name, None), |(stem, ext)| {
        (stem, Some(ext.to_ascii_lowercase()))
    })
}

/// The `order` a container declares, if any.
fn declared_order(doc: &Document, path: &str, faults: &mut Vec<Fault>) -> Option<Vec<String>> {
    #[derive(serde::Deserialize)]
    struct Order {
        #[serde(default)]
        order: Option<Vec<String>>,
    }
    if doc.front.trim().is_empty() {
        return None;
    }
    match serde_yaml::from_str::<Order>(&doc.front) {
        Ok(parsed) => parsed.order,
        Err(err) => {
            faults.push(Fault::file(
                path,
                FaultKind::BadFrontMatter(err.to_string()),
            ));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Blob, Files, Kind, domain_dirs, read_map};

    /// A minimal repository, as the browser holds one: paths and bytes.
    fn repo(pairs: &[(&str, &str)]) -> Files {
        pairs
            .iter()
            .map(|(path, src)| ((*path).to_owned(), Blob::new((*src).as_bytes().to_vec())))
            .collect()
    }

    #[test]
    fn the_domain_folders_come_from_the_paths_alone() {
        let files = repo(&[
            ("index.md", "---\nformat: 1.0.0\norder: [phy, math]\n---\n"),
            ("phy/index.md", "---\nlang: en\n---\n"),
            ("phy/eam/index.md", "---\n---\n"),
            ("math/index.md", "---\nlang: en\n---\n"),
            ("assets/logo.svg", "<svg/>"),
            ("README.md", "hello"),
        ]);
        let found = domain_dirs(files.keys().map(String::as_str));
        assert_eq!(found, ["math", "phy"]);
        // The same folders the reader finds, from the paths with no contents.
        let (root, _) = read_map("", &files);
        let mut read: Vec<String> = root
            .map(|root| root.children.into_iter().map(|node| node.id).collect())
            .unwrap_or_default();
        read.sort();
        assert_eq!(found, read);
    }

    #[test]
    fn a_repository_walks_from_a_map_of_files() {
        let files = repo(&[
            ("index.md", "---\nformat: 1.0.0\norder: [phy]\n---\n"),
            ("phy/index.md", "---\nlang: en\norder: [eam]\n---\n"),
            ("phy/eam/index.md", "---\norder: [fields]\n---\n"),
            ("phy/eam/fields/index.md", "---\norder: [field]\n---\n"),
            (
                "phy/eam/fields/field.md",
                "---\ntitle: A field\n---\n\n::: card\nHello\n:::\n",
            ),
        ]);
        let (root, faults) = read_map("", &files);
        let root = root.expect("the root parses");
        assert!(faults.is_empty(), "{faults:?}");
        assert_eq!(root.kind, Kind::Root);
        let deck = root.walk();
        let deck = deck
            .iter()
            .find(|node| node.kind == Kind::Deck)
            .expect("the deck is reached");
        assert_eq!(deck.id, "field");
        assert_eq!(deck.path, "phy/eam/fields/field.md");
        assert_eq!(deck.dir(), "phy/eam/fields");
    }

    /// The same map read under a prefix, which is how the author's own
    /// directory is walked.
    #[test]
    fn a_root_prefix_is_stripped_from_the_walk() {
        let files = repo(&[
            (
                "content/index.md",
                "---\nformat: 1.0.0\norder: [phy]\n---\n",
            ),
            ("content/phy/index.md", "---\nlang: en\norder: []\n---\n"),
        ]);
        let (root, _) = read_map("content", &files);
        let root = root.expect("the root parses");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].id, "phy");
    }

    #[test]
    fn a_repository_with_no_root_index_says_so() {
        let (root, faults) = read_map("", &repo(&[("phy/index.md", "---\nlang: en\n---\n")]));
        assert!(root.is_none());
        assert_eq!(faults.len(), 1);
    }

    /// An asset beside a node is kept for inlining, and one that is not an
    /// asset at all is named.
    #[test]
    fn assets_are_gathered_and_checked() {
        let mut files = repo(&[
            ("index.md", "---\nformat: 1.0.0\norder: [phy]\n---\n"),
            ("phy/index.md", "---\nlang: en\norder: []\n---\n"),
        ]);
        files.insert("phy/assets/flux.png".to_owned(), Blob::new(vec![0; 10]));
        files.insert("phy/assets/notes.txt".to_owned(), Blob::new(vec![0; 10]));
        let (root, faults) = read_map("", &files);
        let root = root.expect("parses");
        assert_eq!(root.children[0].assets, vec!["phy/assets/flux.png"]);
        assert!(
            faults.iter().any(|f| f.to_string().contains("notes.txt")),
            "{faults:?}"
        );
    }
}
