//! Compile one content repository into per-domain bundles.
//!
//! This is the whole pipeline, and **it runs in the browser**: a learner adds a
//! repository, the app fetches its files, and this turns them into the bundles
//! the app reads. The author's own `stemin check` runs the same code over a
//! directory on disk, so the two can never disagree about what a repository
//! means.
//!
//! It **checks before it renders**: the structural rules of `format::check`
//! first, and nothing renders while one of them fails. Then it renders every
//! body and **carries on past each fault**, so one run names every LaTeX error,
//! every link that resolves to nothing and every missing picture, each with
//! its file and its line.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::de::DeserializeOwned;
use stemin_format::block::{Block, BlockKind, Document};
use stemin_format::{At, Fault, FaultKind, check, front, id, tree};

use crate::error::{BuildError, Result};
use crate::model::{
    BUNDLE_VERSION, Bundle, BundleSection, BundleTopic, CardDef, CheckpointDef, CheckpointEntry,
    CheckpointFolderDef, DeckDef, DomainRef, Edge, EdgeKind, ExerciseDef, GlobalId, LegendDef,
    ReferenceDef,
};
use crate::render::{
    Ctx, Rendered, display_math, escape_attr, escape_text, inline_math, render_body,
};

/// The most HTML one domain compiles to, in bytes, pictures inlined.
///
/// The count holds its cards, exercises, references and checkpoints together.
/// A picture inlines at every place that shows it, so without this cap one
/// picture shown many times would grow the bundle, and the learner's memory,
/// without a bound.
pub const BUNDLE_CAP: usize = BUNDLE_CAP_MB * 1024 * 1024;
/// [`BUNDLE_CAP`] in megabytes, for a message.
pub const BUNDLE_CAP_MB: usize = 32;

/// One repository, ready to compile: where it came from, and what it holds.
pub struct Source<'a> {
    /// The repository's URL. Every domain id is a hash over its canonical form
    /// and the domain's directory name, so this is what makes two repositories
    /// that both hold a `phy` two domains (CONTENT-MODEL.md §6).
    pub url: &'a str,
    /// The repository's files, for the assets a card inlines.
    pub files: &'a tree::Files,
    /// Every domain directory in this repository, in order. An absolute
    /// reference names one of these, and a reference that names anything else
    /// leaves the repository and is refused.
    pub domains: Vec<String>,
    /// Who wrote the repository. A domain that names no author of its own
    /// takes this, so the selector can read "by …" for every domain rather
    /// than only for the ones that repeat themselves.
    pub author: Option<String>,
}

impl Source<'_> {
    /// The id of one of this repository's domains.
    fn id_of(&self, directory: &str) -> Option<String> {
        self.domains
            .iter()
            .any(|name| name == directory)
            .then(|| id::domain_id(self.url, directory))
    }
}

/// Compile every domain in a repository.
///
/// # Errors
/// Returns [`BuildError::Content`] with every fault, each as `file:line:
/// message`, when the repository does not satisfy CONTENT-MODEL.md or a body
/// does not render.
pub fn compile(root: &tree::Node, url: &str, files: &tree::Files) -> Result<Vec<Bundle>> {
    let (bundles, faults) = run(root, url, files);
    if faults.is_empty() {
        return Ok(bundles);
    }
    Err(BuildError::Content(
        faults.iter().map(ToString::to_string).collect(),
    ))
}

/// Every fault the browser's import refuses a repository for, and nothing
/// else: the structural rules, then what only a render finds. This is
/// `stemin check`, after the walk.
#[must_use]
pub fn check(root: &tree::Node, url: &str, files: &tree::Files) -> Vec<Fault> {
    run(root, url, files).1
}

/// Check, then compile every domain, then resolve every link across them.
fn run(root: &tree::Node, url: &str, files: &tree::Files) -> (Vec<Bundle>, Vec<Fault>) {
    let faults = check::check(root);
    if !faults.is_empty() {
        return (Vec::new(), faults);
    }
    let source = Source {
        url,
        files,
        domains: root.children.iter().map(|node| node.id.clone()).collect(),
        author: serde_yaml::from_str::<front::Root>(&root.doc.front)
            .ok()
            .and_then(|declared| declared.author),
    };
    let mut sink = Sink::default();
    let bundles: Vec<Bundle> = root
        .children
        .iter()
        .filter_map(|node| compile_domain(node, &source, &mut sink))
        .collect();
    validate_links(&source, &bundles, &mut sink);
    (bundles, sink.faults)
}

/// What the whole compile collects as it goes.
#[derive(Default)]
struct Sink {
    faults: Vec<Fault>,
    /// Every `reference:` link, with where it sits. A link may point into
    /// another domain, so it resolves once every domain is compiled.
    links: Vec<Placed>,
    /// Where each reference is declared, to place a `uses` cycle.
    declared: HashMap<GlobalId, At>,
}

/// One `reference:` link, and where it sits.
struct Placed {
    predicate: EdgeKind,
    from: GlobalId,
    to: GlobalId,
    written: String,
    at: At,
}

/// Where a body sits in its file, so a fault in it names the real line.
#[derive(Clone, Copy)]
struct Place<'a> {
    path: &'a str,
    /// The file line of each line the body was read from.
    lines: &'a [usize],
    /// How many of those lines precede the body: a reference's `# Title`.
    skip: usize,
    /// The line to name when the body has no such line.
    fallback: usize,
}

impl<'a> Place<'a> {
    /// The body of one block.
    fn block(path: &'a str, block: &'a Block) -> Self {
        Self {
            path,
            lines: &block.body_lines,
            skip: 0,
            fallback: block.line,
        }
    }

    /// The file line of one line of the body, counted from 1.
    fn line(&self, body_line: usize) -> usize {
        stemin_format::block::file_line(self.lines, body_line.saturating_add(self.skip))
            .unwrap_or(self.fallback)
    }
}

/// Where a body's `plot` blocks may go.
#[derive(Clone, Copy)]
enum Plots {
    /// A card or a figure: it draws one plot. The word names it in a fault.
    Draw(&'static str),
    /// Anywhere else: a plot here is never drawn. The words name the place.
    Refuse(&'static str),
}

/// One domain as it compiles.
struct Domain<'s> {
    source: &'s Source<'s>,
    /// The domain id: the hash, never the directory name.
    id: String,
    references: BTreeMap<GlobalId, ReferenceDef>,
    decks: BTreeMap<GlobalId, DeckDef>,
    edges: Vec<Edge>,
    sink: &'s mut Sink,
    /// The bytes of HTML this domain has compiled to so far.
    emitted: usize,
    /// Whether it went past [`BUNDLE_CAP`]. Nothing more renders once it has.
    over: bool,
}

/// Compile one domain into its bundle, or `None` when it holds no deck.
fn compile_domain(node: &tree::Node, source: &Source<'_>, sink: &mut Sink) -> Option<Bundle> {
    let front: front::Domain = parse_front(&node.doc.front);
    // The id is a hash, never the directory name: one repository's domains must
    // not collide with each other, two repositories must not collide at all,
    // and every device derives the same id with no coordination.
    let id = id::domain_id(source.url, &node.id);
    let mut domain = Domain {
        source,
        id: id.clone(),
        references: BTreeMap::new(),
        decks: BTreeMap::new(),
        edges: Vec::new(),
        sink,
        emitted: 0,
        over: false,
    };

    let mut sections: Vec<BundleSection> = Vec::new();
    let mut checkpoints: Vec<CheckpointEntry> = Vec::new();
    for child in &node.children {
        if child.kind == tree::Kind::CheckpointFolder {
            let path = GlobalId::from(id.as_str()).child(&child.id);
            checkpoints = domain.folder(&path, child).entries;
        } else {
            sections.push(domain.section(child));
        }
    }

    if domain.decks.is_empty() {
        domain.sink.faults.push(Fault::file(
            &node.path,
            FaultKind::Render(
                "this domain holds no deck, so it compiles to nothing: \
                 give it a section, a topic and a deck"
                    .to_owned(),
            ),
        ));
        return None;
    }

    Some(Bundle {
        version: BUNDLE_VERSION,
        lang: front.lang,
        labels: front.labels,
        author: front.author.or_else(|| source.author.clone()),
        domain: DomainRef {
            id,
            title: front.title.unwrap_or_else(|| node.id.clone()),
        },
        sections,
        decks: domain.decks,
        references: domain.references,
        checkpoints,
        edges: domain.edges,
    })
}

impl Domain<'_> {
    /// Build one section: its tier vocabulary, then its topics in order.
    fn section(&mut self, node: &tree::Node) -> BundleSection {
        let front: front::Section = parse_front(&node.doc.front);
        let mut topics = Vec::new();
        for child in &node.children {
            let path = GlobalId::from(self.id.as_str())
                .child(&node.id)
                .child(&child.id);
            topics.push(self.topic(&path, child));
        }
        BundleSection {
            id: node.id.clone(),
            title: front.title.unwrap_or_else(|| node.id.clone()),
            tiers: front.tiers,
            topics,
        }
    }

    /// Build one topic: its decks in order, and its `requires` edges. The
    /// checker already holds every `requires` entry to a topic that is there.
    fn topic(&mut self, topic_path: &GlobalId, node: &tree::Node) -> BundleTopic {
        let front: front::Topic = parse_front(&node.doc.front);
        let mut deck_ids: Vec<String> = Vec::new();
        for child in &node.children {
            let deck_global = topic_path.child(&child.id);
            let def = self.deck(&deck_global, child);
            deck_ids.push(child.id.clone());
            self.decks.insert(deck_global, def);
        }
        for reference in &front.requires {
            if let Some(to) = resolve_topic_ref(self.source, reference, topic_path) {
                self.edges
                    .push((topic_path.clone(), EdgeKind::Requires, to));
            }
        }
        BundleTopic {
            id: node.id.clone(),
            title: front.title.unwrap_or_else(|| node.id.clone()),
            tier: front.tier.unwrap_or_default(),
            decks: deck_ids,
        }
    }

    /// Render one deck file into a [`DeckDef`], collecting its `reference`
    /// edges.
    fn deck(&mut self, deck_global: &GlobalId, node: &tree::Node) -> DeckDef {
        let doc = &node.doc;
        let front: front::Deck = parse_front(&doc.front);
        let path = node.path.as_str();

        // Pass 1: number the figures by order. The id (the block's, or `f{n}`)
        // is the stable handle; the number is derived, so reordering never
        // mis-links a reference.
        let mut figures: HashMap<String, usize> = HashMap::new();
        let mut fig_seq: usize = 0;
        for block in &doc.blocks {
            if block.kind == BlockKind::Figure {
                fig_seq = fig_seq.saturating_add(1);
                let id = block.id.clone().unwrap_or_else(|| format!("f{fig_seq}"));
                figures.insert(id, fig_seq);
            }
        }

        let mut cards: Vec<CardDef> = Vec::new();
        let mut exercises: Vec<ExerciseDef> = Vec::new();
        let mut references: Vec<GlobalId> = Vec::new();
        let mut card_no: usize = 0;
        let mut fig_no: usize = 0;
        let mut ex_no: usize = 0;

        for block in &doc.blocks {
            let place = Place::block(path, block);
            match block.kind {
                BlockKind::Card => {
                    card_no = card_no.saturating_add(1);
                    let card_global = deck_global.reference(&card_no.to_string());
                    let (html, plot) = self.body(
                        &card_global,
                        EdgeKind::Reference,
                        place,
                        &block.body,
                        &figures,
                        Plots::Draw("card"),
                    );
                    cards.push(CardDef {
                        rev: rev_hash(&block.body),
                        html,
                        plot,
                    });
                }
                // A figure is its own card: a picture and a book caption, the
                // caption prefixed with its computed "Figure N." label.
                BlockKind::Figure => {
                    card_no = card_no.saturating_add(1);
                    fig_no = fig_no.saturating_add(1);
                    let card_global = deck_global.reference(&card_no.to_string());
                    let (html, plot) = self.body(
                        &card_global,
                        EdgeKind::Reference,
                        place,
                        &block.body,
                        &figures,
                        Plots::Draw("figure"),
                    );
                    let labelled = label_caption(&html, fig_no);
                    cards.push(CardDef {
                        rev: rev_hash(&block.body),
                        html: format!("<div class=\"card-figure\">{labelled}</div>"),
                        plot,
                    });
                }
                BlockKind::Exercise => {
                    ex_no = ex_no.saturating_add(1);
                    let id = block.id.clone().unwrap_or_else(|| format!("e{ex_no}"));
                    let global = deck_global.reference(&id);
                    let exercise = self.exercise(&global, path, block, &figures);
                    exercises.push(ExerciseDef { id, ..exercise });
                }
                BlockKind::Reference => {
                    if let Some(global) = self.reference(path, block) {
                        references.push(global);
                    }
                }
                // The checker refuses every other block at a deck's top level,
                // and nothing compiles while it does.
                _ => {}
            }
        }

        DeckDef {
            title: front.title.unwrap_or_else(|| deck_global.to_string()),
            cards,
            exercises,
            references,
        }
    }

    /// Render one `::: exercise` block: its prompt, answer, and optional
    /// solution. A link anywhere in it, the answer too, is a `reference` edge
    /// from the exercise.
    fn exercise(
        &mut self,
        global: &GlobalId,
        path: &str,
        block: &Block,
        figures: &HashMap<String, usize>,
    ) -> ExerciseDef {
        let link = EdgeKind::Reference;
        let (prompt, _) = self.body(
            global,
            link,
            Place::block(path, block),
            &block.body,
            figures,
            Plots::Refuse("`exercise`"),
        );
        let answer = if let Some(answer) = block.child(BlockKind::Answer) {
            self.body(
                global,
                link,
                Place::block(path, answer),
                &answer.body,
                figures,
                Plots::Refuse("`answer`"),
            )
            .0
        } else {
            self.sink
                .faults
                .push(Fault::line(path, block.line, FaultKind::NoAnswer));
            String::new()
        };
        let solution = block.child(BlockKind::Solution).map(|solution| {
            self.body(
                global,
                link,
                Place::block(path, solution),
                &solution.body,
                figures,
                Plots::Refuse("`solution`"),
            )
            .0
        });

        // The whole exercise: the prompt, the answer and the solution. The rev
        // is what the refresh reads as identity, so a corrected answer must
        // move it.
        let mut whole_exercise = block.body.clone();
        push_blocks(&mut whole_exercise, &block.children);
        ExerciseDef {
            id: String::new(),
            rev: rev_hash(&whole_exercise),
            prompt,
            answer,
            solution,
        }
    }

    /// Render one `::: reference` block of a deck: its `# Title`, its
    /// equation, its legend, its statement and its derivation.
    ///
    /// The links in the statement are `reference` edges, as a card's are. The
    /// links in the derivation are the references it **rests on**: they are
    /// the `uses` edges, so no field has to repeat them.
    fn reference(&mut self, path: &str, block: &Block) -> Option<GlobalId> {
        // The checker holds every reference to an id, a title and one
        // declaration in its domain, and nothing compiles while it does not.
        let id = block.id.as_deref()?;
        let global = GlobalId::from(self.id.as_str()).child(id);
        if self.references.contains_key(&global) {
            return None;
        }
        let (title, statement_md) = stemin_format::block::split_title(&block.body);
        let title = title?;
        let no_figures = HashMap::new();

        let statement = Place {
            skip: stemin_format::block::title_lines(&block.body),
            ..Place::block(path, block)
        };
        let (html, _) = self.body(
            &global,
            EdgeKind::Reference,
            statement,
            &statement_md,
            &no_figures,
            Plots::Refuse("`reference`"),
        );
        let derivation = block.child(BlockKind::Derivation).map(|child| {
            self.body(
                &global,
                EdgeKind::Uses,
                Place::block(path, child),
                &child.body,
                &no_figures,
                Plots::Refuse("`derivation`"),
            )
            .0
        });

        // The bare statement is a `::: equation` block of raw LaTeX, so it
        // goes straight to the math renderer in display mode.
        let equation = match block.child(BlockKind::Equation) {
            Some(child) if !child.body.trim().is_empty() => match display_math(child.body.trim()) {
                Ok(mathml) => {
                    self.spend(path, mathml.len());
                    mathml
                }
                Err(why) => {
                    self.sink.faults.push(Fault::line(
                        path,
                        child.file_line(1),
                        FaultKind::Render(why),
                    ));
                    String::new()
                }
            },
            _ => String::new(),
        };

        // A `::: legend` block is one `symbol: meaning` per line.
        let legend = block
            .child(BlockKind::Legend)
            .map_or_else(Vec::new, |child| self.legend(path, child));

        let mut whole_reference = block.body.clone();
        push_blocks(&mut whole_reference, &block.children);
        self.sink
            .declared
            .insert(global.clone(), At::line(path, block.line));
        self.references.insert(
            global.clone(),
            ReferenceDef {
                title,
                rev: rev_hash(&whole_reference),
                reference: equation,
                legend,
                html,
                derivation,
            },
        );
        Some(global)
    }

    /// Render a `::: legend` block: `$symbol$: meaning`, one per line. The
    /// symbol becomes inline MathML so it reads like the equation above it;
    /// the meaning is escaped, so it is safe text.
    fn legend(&mut self, path: &str, block: &Block) -> Vec<LegendDef> {
        let mut out = Vec::new();
        for (index, line) in block.body.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // The checker refuses a row with no `:`.
            let Some((symbol, meaning)) = line.split_once(':') else {
                continue;
            };
            let symbol = symbol.trim().trim_matches('$');
            match inline_math(symbol) {
                Ok(symbol) => {
                    let meaning = escape_text(meaning.trim());
                    self.spend(path, symbol.len().saturating_add(meaning.len()));
                    out.push(LegendDef { symbol, meaning });
                }
                Err(why) => self.sink.faults.push(Fault::line(
                    path,
                    block.file_line(index.saturating_add(1)),
                    FaultKind::Render(why),
                )),
            }
        }
        out
    }

    /// Build one checkpoint folder: its title, then its folders and
    /// checkpoints, in order.
    fn folder(&mut self, path: &GlobalId, node: &tree::Node) -> CheckpointFolderDef {
        let front: front::CheckpointFolder = parse_front(&node.doc.front);
        let mut entries = Vec::new();
        for child in &node.children {
            let child_path = path.child(&child.id);
            entries.push(if child.kind == tree::Kind::CheckpointFolder {
                CheckpointEntry::Folder(self.folder(&child_path, child))
            } else {
                CheckpointEntry::Checkpoint(self.checkpoint(&child_path, child))
            });
        }
        CheckpointFolderDef {
            id: node.id.clone(),
            title: front.title.unwrap_or_else(|| node.id.clone()),
            entries,
        }
    }

    /// Render one checkpoint: its instructions, then its own questions. The
    /// questions render as exercises do, and nothing schedules them.
    fn checkpoint(&mut self, global: &GlobalId, node: &tree::Node) -> CheckpointDef {
        let doc = &node.doc;
        let front: front::Checkpoint = parse_front(&doc.front);
        let path = node.path.as_str();
        let no_figures = HashMap::new();
        let instructions = Place {
            path,
            lines: &doc.text_lines,
            skip: 0,
            fallback: 0,
        };
        let (html, _) = self.body(
            global,
            EdgeKind::Reference,
            instructions,
            &doc.text,
            &no_figures,
            Plots::Refuse("the instructions of a checkpoint"),
        );

        let mut questions = Vec::new();
        let exercises = doc
            .blocks
            .iter()
            .filter(|block| block.kind == BlockKind::Exercise);
        for (index, block) in exercises.enumerate() {
            let id = block
                .id
                .clone()
                .unwrap_or_else(|| format!("e{}", index.saturating_add(1)));
            let question_global = global.reference(&id);
            let question = self.exercise(&question_global, path, block, &no_figures);
            questions.push(ExerciseDef { id, ..question });
        }

        CheckpointDef {
            id: node.id.clone(),
            title: front.title.unwrap_or_else(|| node.id.clone()),
            rev: rev_hash(&whole(doc)),
            html,
            questions,
        }
    }

    /// Render one body: record its faults at their lines, its links as edges
    /// of one predicate from `unit`, and its plot where a plot may go.
    fn body(
        &mut self,
        unit: &GlobalId,
        predicate: EdgeKind,
        place: Place<'_>,
        md: &str,
        figures: &HashMap<String, usize>,
        plots: Plots,
    ) -> (String, Option<crate::model::Plot>) {
        // Past the cap the compile has failed, so nothing more is rendered.
        if self.over {
            return (String::new(), None);
        }
        let (rendered, cut) = self.render(place.path, md, figures);
        self.spend(place.path, rendered.html.len());
        if cut {
            self.refuse_size(place.path);
        }
        for failure in rendered.failures {
            self.sink.faults.push(Fault::line(
                place.path,
                place.line(failure.line),
                failure.kind,
            ));
        }
        for link in rendered.references {
            let to = GlobalId::new(link.global);
            self.edges.push((unit.clone(), predicate, to.clone()));
            self.sink.links.push(Placed {
                predicate,
                from: unit.clone(),
                to,
                written: link.written,
                at: At::line(place.path, place.line(link.line)),
            });
        }
        let mut drawn = None;
        for (line, plot) in rendered.plots {
            let at = place.line(line);
            match plots {
                Plots::Draw(_) if drawn.is_none() => drawn = Some(plot),
                Plots::Draw(what) => self.sink.faults.push(Fault::line(
                    place.path,
                    at,
                    FaultKind::TwoPlots(what.to_owned()),
                )),
                Plots::Refuse(what) => self.sink.faults.push(Fault::line(
                    place.path,
                    at,
                    FaultKind::PlotPlacement(what.to_owned()),
                )),
            }
        }
        (rendered.html, drawn)
    }

    /// Count bytes of HTML against [`BUNDLE_CAP`], and refuse the domain once
    /// they pass it.
    fn spend(&mut self, path: &str, bytes: usize) {
        self.emitted = self.emitted.saturating_add(bytes);
        if self.emitted > BUNDLE_CAP {
            self.refuse_size(path);
        }
    }

    /// Refuse the domain for its size, once.
    fn refuse_size(&mut self, path: &str) {
        if self.over {
            return;
        }
        self.over = true;
        self.sink.faults.push(Fault::file(
            path,
            FaultKind::Render(format!(
                "the domain's HTML passes its cap of {BUNDLE_CAP_MB} MB in this file: \
                 show each picture in fewer places, or split the domain"
            )),
        ));
    }

    /// Render one body with this domain's links, the deck's figures, and the
    /// repository's assets. Also says whether a picture was left out, because
    /// it would take the domain past [`BUNDLE_CAP`].
    fn render(&self, path: &str, md: &str, figures: &HashMap<String, usize>) -> (Rendered, bool) {
        let source = self.source;
        let domain = self.id.as_str();
        let dir = path.rsplit_once('/').map_or("", |(head, _)| head);
        let mut reference =
            |target: &str| resolve_reference(source, target, domain).map(String::from);
        // A `figure:` link renders the figure's computed number, but links by
        // the stable id, so inserting or reordering figures never mis-links.
        let mut figure = |id: &str| {
            figures.get(id).map_or_else(
                || {
                    Err(FaultKind::Dangling {
                        target: format!("figure:{id}"),
                        kind: "figure",
                        scope: "this deck".to_owned(),
                    })
                },
                |number| {
                    Ok(format!(
                        "<a class=\"figure-ref\" data-figure=\"{}\">Figure {number}</a>",
                        escape_attr(id)
                    ))
                },
            )
        };
        // The pictures of this body may take what the domain has left. One
        // that does not fit is left out, and the caller refuses the domain.
        let budget = BUNDLE_CAP.saturating_sub(self.emitted);
        let mut spent: usize = 0;
        let mut cut = false;
        let mut asset = |dest: &str, alt: &str| {
            if cut {
                return Ok(String::new());
            }
            let markup = inline_asset(source, dir, dest, alt).map_err(FaultKind::Render)?;
            spent = spent.saturating_add(markup.len());
            if spent > budget {
                cut = true;
                return Ok(String::new());
            }
            Ok(markup)
        };
        let rendered = render_body(
            md,
            &mut Ctx {
                reference: &mut reference,
                figure: &mut figure,
                asset: &mut asset,
            },
        );
        (rendered, cut)
    }
}

/// Name the caption of a figure card: `Figure 3.`, in the paragraph that reads
/// rather than the one that draws.
///
/// The picture sits in a paragraph of its own, so labelling the first paragraph
/// would put the number on the picture and leave the caption bare.
fn label_caption(html: &str, number: usize) -> String {
    let label = format!("<p class=\"fig-cap\"><span class=\"fig-num\">Figure {number}.</span> ");
    let mut out = String::with_capacity(html.len().saturating_add(label.len()));
    let mut rest = html;
    let mut named = false;
    while let Some(at) = rest.find("<p>") {
        let (head, tail) = rest.split_at(at);
        let end = tail
            .find("</p>")
            .map_or(tail.len(), |e| e.saturating_add(4));
        let (paragraph, after) = tail.split_at(end);
        out.push_str(head);
        let draws = paragraph.contains("<svg") || paragraph.contains("<img");
        if named || draws {
            out.push_str(paragraph);
        } else {
            out.push_str(&paragraph.replacen("<p>", &label, 1));
            named = true;
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Inline one asset, resolved relative to the file that references it. An
/// asset may sit at any level, so a deck reaches a section's pictures with
/// `../../assets/x.png`.
fn inline_asset(
    source: &Source<'_>,
    dir: &str,
    dest: &str,
    alt: &str,
) -> std::result::Result<String, String> {
    if dest.contains("://") || dest.starts_with("//") {
        return Err(format!(
            "`{dest}` is a link to another site, and a card fetches nothing: \
             commit the picture under `assets/`"
        ));
    }
    let joined = if dir.is_empty() {
        dest.to_owned()
    } else {
        format!("{dir}/{dest}")
    };
    let path = normalize(&joined).ok_or_else(|| format!("`{dest}` leaves the repository"))?;
    let blob = source
        .files
        .get(&path)
        .ok_or_else(|| format!("`{dest}` is not in this repository"))?;
    // The walk refuses an asset past the cap, but the compile holds to it on
    // its own, whatever map of files it is given.
    let size = blob
        .size
        .max(u64::try_from(blob.bytes.len()).unwrap_or(u64::MAX));
    if size > tree::ASSET_CAP {
        return Err(format!(
            "`{dest}` is {size} bytes, over the {}-byte cap: shrink it before committing it",
            tree::ASSET_CAP
        ));
    }
    crate::asset::inline(&path, &blob.bytes, alt)
}

/// A `/`-separated path with its `.` and `..` segments resolved. `None` when a
/// `..` climbs above the first segment.
fn normalize(path: &str) -> Option<String> {
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    Some(if absolute {
        format!("/{joined}")
    } else {
        joined
    })
}

/// Everything a file says, for a `rev`.
///
/// The prose alone is not enough: a reference keeps its equation, its legend
/// and its derivation in blocks, so hashing `doc.text` would leave `rev`
/// unchanged when any of them is edited.
fn whole(doc: &Document) -> String {
    let mut out = String::from(&doc.front);
    out.push_str(&doc.text);
    push_blocks(&mut out, &doc.blocks);
    out
}

/// Append every block's kind, id and body, depth first.
fn push_blocks(out: &mut String, blocks: &[Block]) {
    for block in blocks {
        out.push_str(block.kind.as_str());
        out.push_str(block.id.as_deref().unwrap_or_default());
        out.push_str(&block.body);
        push_blocks(out, &block.children);
    }
}

/// First 8 hex characters of the BLAKE3 hash of a unit's source.
fn rev_hash(src: &str) -> String {
    blake3::hash(src.as_bytes()).to_hex()[..8].to_owned()
}

/// Parse a node's YAML front matter. The checker already parsed it, and
/// nothing compiles while it fails, so the default is never reached in use.
fn parse_front<T: DeserializeOwned + Default>(front: &str) -> T {
    if front.trim().is_empty() {
        return T::default();
    }
    serde_yaml::from_str(front).unwrap_or_default()
}

/// Resolve a reference (`id` in this domain, or `/domain/id` in another of
/// this repository's) to a global id.
///
/// An absolute reference names a **directory** in this repository, and the id
/// it resolves to is that directory's hash. A reference to anything else
/// leaves the repository: the other repository may not be installed, and its
/// id is a hash the author cannot write down, so it is refused rather than
/// emitted as a dangling edge (CONTENT-MODEL.md §6).
fn resolve_reference(
    source: &Source<'_>,
    reference: &str,
    domain: &str,
) -> std::result::Result<GlobalId, FaultKind> {
    let Some(rest) = reference.strip_prefix('/') else {
        return Ok(GlobalId::from(domain).child(reference));
    };
    let written = format!("reference:{reference}");
    let Some((directory, tail)) = rest.split_once('/') else {
        return Err(FaultKind::Dangling {
            target: written,
            kind: "reference",
            scope: "this repository".to_owned(),
        });
    };
    let id = source
        .id_of(directory)
        .ok_or(FaultKind::ForeignReference(written))?;
    Ok(GlobalId::from(id.as_str()).child(tail))
}

/// Resolve a `requires` topic reference (`bare` in the same section, or
/// `/domain/section/topic`). `None` for one that leaves the repository, which
/// the checker already refused.
fn resolve_topic_ref(source: &Source<'_>, reference: &str, context: &GlobalId) -> Option<GlobalId> {
    if let Some(rest) = reference.strip_prefix('/') {
        let (directory, tail) = rest.split_once('/')?;
        let id = source.id_of(directory)?;
        return Some(GlobalId::from(id.as_str()).child(tail));
    }
    // `fundamentals` → the same section: replace the last path segment.
    let parent = context.parent()?;
    Some(GlobalId::from(parent).child(reference))
}

/// Every `reference:` link names a reference that is there, in its own domain
/// or in another of this repository's, and the `uses` graph has no cycle.
fn validate_links(source: &Source<'_>, bundles: &[Bundle], sink: &mut Sink) {
    let known: BTreeSet<&GlobalId> = bundles
        .iter()
        .flat_map(|bundle| bundle.references.keys())
        .collect();
    // A cycle reads in the author's own words, `/phy/ohms-law`, never a hash.
    let dirs: HashMap<String, &str> = source
        .domains
        .iter()
        .map(|dir| (id::domain_id(source.url, dir), dir.as_str()))
        .collect();
    let named = |global: &GlobalId| -> String {
        let (domain, rest) = global
            .as_str()
            .split_once('/')
            .unwrap_or((global.as_str(), ""));
        let dir = dirs.get(domain).copied().unwrap_or(domain);
        format!("/{dir}/{rest}")
    };

    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut found: Vec<Fault> = Vec::new();
    for link in &sink.links {
        if !known.contains(&link.to) {
            let scope = link
                .written
                .strip_prefix("reference:/")
                .and_then(|rest| rest.split_once('/'))
                .map_or_else(
                    || "this domain".to_owned(),
                    |(dir, _)| format!("domain `{dir}`"),
                );
            found.push(Fault {
                at: link.at.clone(),
                kind: FaultKind::Dangling {
                    target: link.written.clone(),
                    kind: "reference",
                    scope,
                },
            });
            continue;
        }
        if link.predicate == EdgeKind::Uses {
            graph
                .entry(named(&link.from))
                .or_default()
                .push(named(&link.to));
        }
    }
    let places: HashMap<String, &At> = sink
        .declared
        .iter()
        .map(|(global, at)| (named(global), at))
        .collect();
    for cycle in check::cycles(&graph) {
        let at = cycle
            .first()
            .and_then(|first| places.get(first))
            .map_or_else(|| At::file(""), |at| (*at).clone());
        found.push(Fault {
            at,
            kind: FaultKind::Cycle {
                predicate: "uses",
                path: cycle,
            },
        });
    }
    sink.faults.extend(found);
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn an_asset_path_climbs_to_any_level_and_no_further() {
        assert_eq!(
            normalize("math/alg/linears/../assets/a.png").as_deref(),
            Some("math/alg/assets/a.png")
        );
        assert_eq!(
            normalize("math/alg/linears/./assets/a.png").as_deref(),
            Some("math/alg/linears/assets/a.png")
        );
        assert_eq!(
            normalize("/tmp/repo/math/../assets/a.png").as_deref(),
            Some("/tmp/repo/assets/a.png")
        );
        assert_eq!(normalize("math/../../a.png"), None);
    }
}
