//! The structural rules: everything a repository can get wrong that needs no
//! renderer.
//!
//! Every rule reports and carries on, so one run tells an author everything they
//! have to fix rather than the first thing. `stemin check` runs these, then the
//! compile the browser runs, which adds what only a render finds: math, links
//! and pictures (`core::compile`).

use std::collections::{BTreeMap, HashMap};

use serde::de::DeserializeOwned;

use crate::block::{Block, BlockKind};
use crate::error::{Fault, FaultKind};
use crate::front::{READS_MAJOR, Root, Version};
use crate::tree::{Kind, Node};

/// Ids the app routes on, which content may not take.
///
/// This is the **one** list. `core`'s build reads it too, so a repository whose
/// own CI goes green can never be refused by the build afterwards, which is the
/// drift the `format` crate exists to remove.
pub const RESERVED: [&str; 11] = [
    "index",
    "today",
    "practice",
    "account",
    "cards",
    "checkpoint",
    "checkpoints",
    "exam",
    "exams",
    "reference",
    "test",
];

/// Check a repository that [`crate::tree::read`] already parsed.
///
/// Returns every fault, most structural first, so the list reads top down.
#[must_use]
pub fn check(root: &Node) -> Vec<Fault> {
    let mut faults = Vec::new();
    check_format(root, &mut faults);
    for node in root.walk() {
        check_order(node, &mut faults);
        check_reserved(node, &mut faults);
        check_blocks(node, &mut faults);
        check_answers(node, &mut faults);
        check_plots(node, &mut faults);
        check_loose_text(node, &mut faults);
        check_local_ids(node, &mut faults);
        check_empty(node, &mut faults);
        check_front(node, &mut faults);
        check_legends(node, &mut faults);
        check_tiers(node, &mut faults);
    }
    check_unique_ids(root, &mut faults);
    check_references(root, &mut faults);
    check_requires(root, &mut faults);
    faults
}

/// The root declares a format version this build can read.
fn check_format(root: &Node, faults: &mut Vec<Fault>) {
    let parsed = serde_yaml::from_str::<Root>(&root.doc.front);
    let Ok(declared) = parsed else {
        faults.push(Fault::file(
            &root.path,
            FaultKind::FormatMalformed("this repository declares none".to_owned()),
        ));
        return;
    };
    match Version::parse(&declared.format) {
        Ok(version) if version.readable() => {}
        Ok(version) => faults.push(Fault::file(
            &root.path,
            FaultKind::FormatMajor {
                found: version.to_string(),
                readable: READS_MAJOR,
            },
        )),
        Err(why) => faults.push(Fault::file(&root.path, FaultKind::FormatMalformed(why))),
    }
}

/// `order` is a bijection with the directory, both ways.
fn check_order(node: &Node, faults: &mut Vec<Fault>) {
    for id in &node.missing {
        faults.push(Fault::file(&node.path, FaultKind::OrderMissing(id.clone())));
    }
    for id in &node.unlisted {
        faults.push(Fault::file(
            &node.path,
            FaultKind::OrderUnlisted(id.clone()),
        ));
    }
}

/// No content takes an id the app routes on. A checkpoint and its folders sit
/// under their own route segment, so their ids are free.
fn check_reserved(node: &Node, faults: &mut Vec<Fault>) {
    let routed = !matches!(
        node.kind,
        Kind::Root | Kind::CheckpointFolder | Kind::Checkpoint
    );
    if routed && RESERVED.contains(&node.id.as_str()) {
        faults.push(Fault::file(
            &node.path,
            FaultKind::ReservedId(node.id.clone()),
        ));
    }
}

/// Every block sits where the grammar allows, and in a file that holds it.
///
/// Both halves matter. `stands_alone` says a block may sit at a top level;
/// [`belongs`] says **which** file's top level. Without the second, a deck
/// carrying an `::: equation` passed the checker and was then refused by the
/// build, which is the drift this crate exists to remove.
fn check_blocks(node: &Node, faults: &mut Vec<Fault>) {
    for block in &node.doc.blocks {
        if !block.kind.stands_alone() {
            faults.push(Fault::line(
                &node.path,
                block.line,
                FaultKind::OrphanBlock(block.kind.to_string()),
            ));
        } else if !belongs(node.kind, block.kind) {
            faults.push(Fault::line(
                &node.path,
                block.line,
                FaultKind::MisplacedBlock {
                    block: block.kind.to_string(),
                    parent: node.kind.as_str().to_owned(),
                },
            ));
        }
        check_nesting(node, block, faults);
    }
}

/// A block holds only what its kind allows.
fn check_nesting(node: &Node, block: &crate::block::Block, faults: &mut Vec<Fault>) {
    for child in &block.children {
        if !block.kind.holds(child.kind) {
            faults.push(Fault::line(
                &node.path,
                child.line,
                FaultKind::MisplacedBlock {
                    block: child.kind.to_string(),
                    parent: block.kind.to_string(),
                },
            ));
        }
        check_nesting(node, child, faults);
    }
}

/// Every block, at any depth, with the file it sits in.
fn all_blocks(blocks: &[Block]) -> Vec<&Block> {
    let mut out = Vec::new();
    for block in blocks {
        out.push(block);
        out.extend(all_blocks(&block.children));
    }
    out
}

/// Every exercise has an `::: answer`: the back of its flashcard.
fn check_answers(node: &Node, faults: &mut Vec<Fault>) {
    for block in all_blocks(&node.doc.blocks) {
        if block.kind == BlockKind::Exercise && block.child(BlockKind::Answer).is_none() {
            faults.push(Fault::line(&node.path, block.line, FaultKind::NoAnswer));
        }
    }
}

/// Every ```` ```plot ```` fence is a plot this build can draw, in a place the
/// app draws it, and a card draws one plot at most.
///
/// The one fenced language is the only part of a file the block grammar does
/// not already cover, and a plot that will not parse draws nothing at all. The
/// app draws the plot of a card or a figure and drops every other one, so a
/// plot anywhere else is an error here, where an author sees it, rather than a
/// hole a learner sees.
fn check_plots(node: &Node, faults: &mut Vec<Fault>) {
    for block in all_blocks(&node.doc.blocks) {
        let draws = matches!(block.kind, BlockKind::Card | BlockKind::Figure);
        for (count, (offset, body)) in crate::block::fences(&block.body, crate::block::PLOT)
            .into_iter()
            .enumerate()
        {
            let line = block.file_line(offset);
            if let Err(why) = crate::plot::parse(&body) {
                faults.push(Fault::line(&node.path, line, FaultKind::BadPlot(why)));
            }
            if !draws {
                faults.push(Fault::line(
                    &node.path,
                    line,
                    FaultKind::PlotPlacement(format!("`{}`", block.kind)),
                ));
            } else if count > 0 {
                faults.push(Fault::line(
                    &node.path,
                    line,
                    FaultKind::TwoPlots(block.kind.to_string()),
                ));
            }
        }
    }
    // A checkpoint's loose text is its instructions, which draw no plot. In
    // any other file the loose text is an error of its own.
    if node.kind == Kind::Checkpoint {
        for (offset, _) in crate::block::fences(&node.doc.text, crate::block::PLOT) {
            let line = crate::block::file_line(&node.doc.text_lines, offset).unwrap_or(0);
            faults.push(Fault::line(
                &node.path,
                line,
                FaultKind::PlotPlacement("the instructions of a checkpoint".to_owned()),
            ));
        }
    }
}

/// Text outside any block reaches no learner. A checkpoint is the exception:
/// its loose text is its instructions.
fn check_loose_text(node: &Node, faults: &mut Vec<Fault>) {
    let hint = match node.kind {
        Kind::Checkpoint => return,
        Kind::Deck => "put it in a `::: card`, or take it out",
        Kind::Root | Kind::Domain | Kind::Section | Kind::Topic | Kind::CheckpointFolder => {
            "an `index.md` holds front matter and nothing else: take it out"
        }
    };
    let first = node
        .doc
        .text
        .lines()
        .zip(node.doc.text_lines.iter())
        .find(|(text, _)| !text.trim().is_empty());
    if let Some((_, line)) = first {
        faults.push(Fault::line(&node.path, *line, FaultKind::LooseText(hint)));
    }
}

/// No two exercises, and no two figures, in one file share an id. An exercise
/// with no id is `e1`, `e2` by its place among the exercises, and a figure
/// with none is `f1`, `f2`, so a written `e2` can collide with a derived one.
fn check_local_ids(node: &Node, faults: &mut Vec<Fault>) {
    for (kind, what, prefix) in [
        (BlockKind::Exercise, "exercise", 'e'),
        (BlockKind::Figure, "figure", 'f'),
    ] {
        let mut seen: HashMap<String, usize> = HashMap::new();
        let blocks = node.doc.blocks.iter().filter(|block| block.kind == kind);
        for (index, block) in blocks.enumerate() {
            let id = block
                .id
                .clone()
                .unwrap_or_else(|| format!("{prefix}{}", index.saturating_add(1)));
            if let Some(line) = seen.get(&id) {
                faults.push(Fault::line(
                    &node.path,
                    block.line,
                    FaultKind::DuplicateLocal {
                        id,
                        what,
                        line: *line,
                    },
                ));
            } else {
                seen.insert(id, block.line);
            }
        }
    }
}

/// A container holds something.
fn check_empty(node: &Node, faults: &mut Vec<Fault>) {
    let is_container = node.kind.is_container();
    if is_container && node.children.is_empty() {
        faults.push(Fault::file(
            &node.path,
            FaultKind::EmptyContainer(node.kind.as_str()),
        ));
    }
}

/// Every `::: legend` row splits into a symbol and a meaning.
///
/// The renderer skips a row it cannot split, so without this an author loses a
/// legend line to a typo with no signal anywhere.
fn check_legends(node: &Node, faults: &mut Vec<Fault>) {
    // A legend sits inside its reference, so the walk goes one level down.
    let legends = node
        .doc
        .blocks
        .iter()
        .flat_map(|block| std::iter::once(block).chain(block.children.iter()))
        .filter(|block| block.kind == BlockKind::Legend);
    for block in legends {
        for (offset, line) in block.body.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.contains(':') {
                continue;
            }
            faults.push(Fault::line(
                &node.path,
                block.line.saturating_add(offset).saturating_add(1),
                FaultKind::BadLegendRow(line.to_owned()),
            ));
        }
    }
}

/// A node's own front matter parses, holds only the keys its kind takes, and
/// a domain declares a language.
///
/// A key the format does not read used to vanish in silence, so a misspelt
/// `requries:` cost a topic its prerequisites with no signal anywhere.
fn check_front(node: &Node, faults: &mut Vec<Fault>) {
    let front = node.doc.front.trim();
    // The walk already read a container's `order`, and reported its YAML when
    // it would not parse. The root's `format:` has a rule of its own.
    let reported = node.kind.is_container() && order_fails(front);
    let parsed = if front.is_empty() {
        Ok(serde_yaml::Value::Null)
    } else {
        serde_yaml::from_str::<serde_yaml::Value>(front)
    };
    let mapping = match parsed {
        Ok(serde_yaml::Value::Mapping(mapping)) => mapping,
        // Front matter of comments alone says nothing, as none does.
        Ok(serde_yaml::Value::Null) => serde_yaml::Mapping::new(),
        _ if reported => return,
        Ok(_) => {
            faults.push(Fault::file(
                &node.path,
                FaultKind::BadFrontMatter("it must be `key: value` lines".to_owned()),
            ));
            return;
        }
        Err(err) => {
            faults.push(Fault::file(
                &node.path,
                FaultKind::BadFrontMatter(err.to_string()),
            ));
            return;
        }
    };
    let allowed = crate::front::keys(node.kind);
    for key in mapping.keys() {
        let name = key.as_str().unwrap_or_default();
        if !allowed.contains(&name) {
            faults.push(Fault::file(
                &node.path,
                FaultKind::UnknownKey {
                    key: yaml_text(key),
                    kind: node.kind.as_str(),
                    allowed: allowed.join(", "),
                },
            ));
        }
    }
    if reported {
        return;
    }
    let parsed = match node.kind {
        // The root's `format:` is read by `check_format`.
        Kind::Root => Ok(()),
        Kind::Domain => check_domain(node, &mapping, faults),
        Kind::Section => typed::<crate::front::Section>(front),
        Kind::Topic => typed::<crate::front::Topic>(front),
        Kind::Deck => typed::<crate::front::Deck>(front),
        Kind::CheckpointFolder => typed::<crate::front::CheckpointFolder>(front),
        Kind::Checkpoint => typed::<crate::front::Checkpoint>(front),
    };
    if let Err(why) = parsed {
        faults.push(Fault::file(&node.path, FaultKind::BadFrontMatter(why)));
    }
}

/// A domain declares a language, and names only labels the format knows.
fn check_domain(
    node: &Node,
    mapping: &serde_yaml::Mapping,
    faults: &mut Vec<Fault>,
) -> Result<(), String> {
    let lang = mapping.get("lang").and_then(serde_yaml::Value::as_str);
    if lang.is_none_or(|lang| lang.trim().is_empty()) {
        faults.push(Fault::file(&node.path, FaultKind::BadLang));
        return Ok(());
    }
    if let Some(serde_yaml::Value::Mapping(labels)) = mapping.get("labels") {
        for key in labels.keys() {
            let name = key.as_str().unwrap_or_default();
            if !crate::front::LABELS.iter().any(|(known, _)| *known == name) {
                faults.push(Fault::file(
                    &node.path,
                    FaultKind::UnknownLabel(yaml_text(key)),
                ));
            }
        }
    }
    typed::<crate::front::Domain>(&node.doc.front)
}

/// Parse front matter as its kind's type, for a fault that says what is wrong.
fn typed<T: DeserializeOwned>(front: &str) -> Result<(), String> {
    if front.trim().is_empty() {
        return Ok(());
    }
    serde_yaml::from_str::<T>(front)
        .map(drop)
        .map_err(|err| err.to_string())
}

/// Whether the walk already refused this container's front matter.
fn order_fails(front: &str) -> bool {
    #[derive(serde::Deserialize)]
    struct Order {
        #[serde(default)]
        #[expect(dead_code, reason = "read only to see whether it parses")]
        order: Option<Vec<String>>,
    }
    !front.is_empty() && serde_yaml::from_str::<Order>(front).is_err()
}

/// A YAML key, as an author wrote it.
fn yaml_text(key: &serde_yaml::Value) -> String {
    key.as_str().map_or_else(
        || {
            serde_yaml::to_string(key)
                .map(|text| text.trim().to_owned())
                .unwrap_or_default()
        },
        str::to_owned,
    )
}

/// A topic's `tier` is one of its section's `tiers`.
fn check_tiers(node: &Node, faults: &mut Vec<Fault>) {
    if node.kind != Kind::Section {
        return;
    }
    // A front matter that will not parse is a fault of its own, and its
    // default would read as a list of no tiers.
    let Some(section) = parse_some::<crate::front::Section>(&node.doc.front) else {
        return;
    };
    let tiers = section.tiers;
    for topic in &node.children {
        let Some(tier) = parse_some::<crate::front::Topic>(&topic.doc.front).and_then(|t| t.tier)
        else {
            continue;
        };
        if !tiers.contains(&tier) {
            faults.push(Fault::file(
                &topic.path,
                FaultKind::TierUnknown {
                    tier,
                    tiers: format!("[{}]", tiers.join(", ")),
                },
            ));
        }
    }
}

/// Front matter as its type, or `None` where it will not parse. Its parse
/// fault is reported by [`check_front`].
fn parse_some<T: DeserializeOwned + Default>(front: &str) -> Option<T> {
    if front.trim().is_empty() {
        return Some(T::default());
    }
    serde_yaml::from_str(front).ok()
}

/// Front matter as its type, or the default where it will not parse. Its
/// parse fault is reported by [`check_front`].
fn parse_or_default<T: DeserializeOwned + Default>(front: &str) -> T {
    if front.trim().is_empty() {
        return T::default();
    }
    serde_yaml::from_str(front).unwrap_or_default()
}

/// Every `requires` entry names a topic in this repository, and the
/// `requires` graph has no cycle.
///
/// A bare id names a topic of the same section. `/domain/section/topic` names
/// any topic of this repository. A domain this repository does not hold is
/// another repository, which a link cannot reach (CONTENT-MODEL.md §6).
fn check_requires(root: &Node, faults: &mut Vec<Fault>) {
    // Every topic, as `/domain/section/topic`, with its file.
    let mut topics: BTreeMap<String, &Node> = BTreeMap::new();
    let mut sections_of: Vec<(String, &Node)> = Vec::new();
    for domain in &root.children {
        for section in domain.children.iter().filter(|n| n.kind == Kind::Section) {
            for topic in &section.children {
                let key = format!("/{}/{}/{}", domain.id, section.id, topic.id);
                topics.insert(key.clone(), topic);
                sections_of.push((format!("/{}/{}", domain.id, section.id), topic));
            }
        }
    }
    let domains: Vec<&str> = root.children.iter().map(|n| n.id.as_str()).collect();
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (section, topic) in &sections_of {
        let from = format!("{section}/{}", topic.id);
        let front = parse_or_default::<crate::front::Topic>(&topic.doc.front);
        for target in &front.requires {
            let resolved = if let Some(rest) = target.strip_prefix('/') {
                let dir = rest.split('/').next().unwrap_or_default();
                if !domains.contains(&dir) {
                    faults.push(Fault::file(
                        &topic.path,
                        FaultKind::ForeignReference(target.clone()),
                    ));
                    continue;
                }
                (target.clone(), format!("domain `{dir}`"))
            } else {
                (format!("{section}/{target}"), "this section".to_owned())
            };
            let (key, scope) = resolved;
            if target.is_empty() || !topics.contains_key(&key) {
                faults.push(Fault::file(
                    &topic.path,
                    FaultKind::Dangling {
                        target: target.clone(),
                        kind: "topic",
                        scope,
                    },
                ));
                continue;
            }
            graph.entry(from.clone()).or_default().push(key);
        }
    }
    for cycle in cycles(&graph) {
        let at = cycle
            .first()
            .and_then(|first| topics.get(first))
            .map_or(String::new(), |node| node.path.clone());
        faults.push(Fault::file(
            at,
            FaultKind::Cycle {
                predicate: "requires",
                path: cycle,
            },
        ));
    }
}

/// The most cycles one graph reports. Past this an author has a tangle to
/// undo, not a list to read, and a crafted graph cannot make the report grow
/// with the square of its size.
pub const MAX_CYCLES: usize = 32;

/// Every cycle in a graph, each once, as the path that closes it: `a, b, a`.
///
/// The walk is depth first with an explicit stack, not recursion, so a chain
/// of any length cannot overflow the stack. It reports [`MAX_CYCLES`] at most.
#[must_use]
pub fn cycles(graph: &BTreeMap<String, Vec<String>>) -> Vec<Vec<String>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Visit {
        /// On the current path, at this place in it.
        Active(usize),
        Done,
    }

    let mut state: HashMap<&str, Visit> = HashMap::new();
    let mut found: Vec<Vec<String>> = Vec::new();
    // The current path: each node, with the index of the next edge to follow.
    let mut path: Vec<(&str, usize)> = Vec::new();
    for start in graph.keys() {
        if state.contains_key(start.as_str()) {
            continue;
        }
        state.insert(start, Visit::Active(0));
        path.push((start, 0));
        while let Some(top) = path.last_mut() {
            let (node, index) = *top;
            top.1 = index.saturating_add(1);
            let Some(next) = graph.get(node).and_then(|edges| edges.get(index)) else {
                state.insert(node, Visit::Done);
                path.pop();
                continue;
            };
            let next = next.as_str();
            match state.get(next) {
                Some(Visit::Done) => {}
                Some(Visit::Active(at)) => {
                    if found.len() < MAX_CYCLES {
                        let mut cycle: Vec<String> = path
                            .iter()
                            .skip(*at)
                            .map(|(step, _)| (*step).to_owned())
                            .collect();
                        cycle.push(next.to_owned());
                        found.push(cycle);
                    }
                }
                None => {
                    state.insert(next, Visit::Active(path.len()));
                    path.push((next, 0));
                }
            }
        }
    }
    found
}

/// Two files never claim one id, per domain and per kind. A checkpoint is the
/// exception: its route carries its folders, so two universities may both
/// publish a `2020`.
fn check_unique_ids(root: &Node, faults: &mut Vec<Fault>) {
    for domain in &root.children {
        let mut seen: HashMap<(Kind, &str), &Node> = HashMap::new();
        for node in domain.walk() {
            if node.id.is_empty() || matches!(node.kind, Kind::CheckpointFolder | Kind::Checkpoint)
            {
                continue;
            }
            let key = (node.kind, node.id.as_str());
            if let Some(other) = seen.insert(key, node) {
                faults.push(Fault::file(
                    &node.path,
                    FaultKind::DuplicateId {
                        id: node.id.clone(),
                        kind: node.kind.as_str(),
                        other: other.path.clone(),
                    },
                ));
            }
        }
    }
}

/// Every reference names itself with an id and a `# Title`, and no two
/// references in a domain share an id: a `reference:` link reaches one from any
/// deck of the domain, so the id must pick exactly one.
fn check_references(root: &Node, faults: &mut Vec<Fault>) {
    for domain in &root.children {
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for node in domain.walk() {
            for block in &node.doc.blocks {
                if block.kind != BlockKind::Reference {
                    continue;
                }
                let Some(id) = block.id.as_deref() else {
                    faults.push(Fault::line(
                        &node.path,
                        block.line,
                        FaultKind::ReferenceNoId,
                    ));
                    continue;
                };
                if crate::block::split_title(&block.body).0.is_none() {
                    faults.push(Fault::line(
                        &node.path,
                        block.line,
                        FaultKind::ReferenceNoTitle(id.to_owned()),
                    ));
                }
                if let Some(other) = seen.insert(id, &node.path) {
                    faults.push(Fault::line(
                        &node.path,
                        block.line,
                        FaultKind::DuplicateId {
                            id: id.to_owned(),
                            kind: "reference",
                            other: other.to_owned(),
                        },
                    ));
                }
            }
        }
    }
}

/// Whether a block kind belongs in a file of this kind. This is the same rule
/// `core`'s build enforces when it renders, and the checker must agree with it.
///
/// A deck is the only thing an author teaches with: its cards, its exercises
/// and its references. A checkpoint holds questions of its own.
#[must_use]
pub const fn belongs(kind: Kind, block: BlockKind) -> bool {
    match kind {
        Kind::Deck => matches!(
            block,
            BlockKind::Card | BlockKind::Figure | BlockKind::Exercise | BlockKind::Reference
        ),
        Kind::Checkpoint => matches!(block, BlockKind::Exercise),
        Kind::Root | Kind::Domain | Kind::Section | Kind::Topic | Kind::CheckpointFolder => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{MAX_CYCLES, cycles};

    /// A chain of `n` nodes, each pointing at the next.
    fn chain(n: usize) -> BTreeMap<String, Vec<String>> {
        (0..n)
            .map(|i| {
                (
                    format!("n{i:07}"),
                    vec![format!("n{:07}", i.saturating_add(1))],
                )
            })
            .collect()
    }

    /// The audit's M3 probe: a chain far longer than any stack. The walk
    /// holds its own stack, so it ends, and finds the one cycle that closes
    /// the chain.
    #[test]
    fn a_long_chain_cannot_overflow_the_stack() {
        let mut graph = chain(200_000);
        assert!(cycles(&graph).is_empty());
        graph.insert("n0200000".to_owned(), vec!["n0000000".to_owned()]);
        let found = cycles(&graph);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].len(), 200_002);
        assert_eq!(found[0].first(), found[0].last());
    }

    /// Each cycle is reported once, as the path that closes it.
    #[test]
    fn a_cycle_reads_as_its_path() {
        let graph: BTreeMap<String, Vec<String>> =
            [("a", vec!["b"]), ("b", vec!["c", "a"]), ("c", vec!["c"])]
                .into_iter()
                .map(|(from, to)| (from.to_owned(), to.into_iter().map(str::to_owned).collect()))
                .collect();
        assert_eq!(cycles(&graph), vec![vec!["c", "c"], vec!["a", "b", "a"]]);
    }

    /// A graph of many cycles reports a bounded number of them.
    #[test]
    fn the_report_holds_a_bounded_number_of_cycles() {
        let graph: BTreeMap<String, Vec<String>> = (0..1000)
            .map(|i| (format!("n{i:04}"), vec![format!("n{i:04}")]))
            .collect();
        assert_eq!(cycles(&graph).len(), MAX_CYCLES);
    }
}
