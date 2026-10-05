//! The block grammar: front matter, `::: kind` blocks, and the two fenced
//! languages.
//!
//! A file is optional YAML front matter, then nested `::: kind` … `:::` blocks
//! and free text between them. This module knows the **shape** of a file and
//! nothing about rendering it, so it builds for `wasm32` and the checker carries
//! no renderer (CONTENT-MODEL.md §10).

use crate::error::FaultKind;

/// The grammar, as one line, for an error message.
pub const GRAMMAR: &str =
    "card, figure, exercise, answer, solution, reference, equation, legend, derivation";

/// The one fenced language: a live figure the app draws (CONTENT-MODEL.md
/// §10.1). A still one is an author's SVG asset.
pub const PLOT: &str = "plot";

/// How deep blocks nest: a top-level block, and one level of children in it.
///
/// The grammar needs no more (an `answer` in an `exercise`, a `legend` in a
/// `reference`). A deeper tree is refused at parse time, so no later walk over
/// the blocks can recurse without a bound.
pub const MAX_NESTING: usize = 2;

/// Every fenced block of one language in a body, as `(line, body)`.
///
/// The line counts from 1 **within this body**, so a caller adds the block's
/// own line to place a fault in the file.
#[must_use]
pub fn fences(body: &str, language: &str) -> Vec<(usize, String)> {
    /// The fence being read.
    struct Open {
        marker: String,
        wanted: bool,
        line: usize,
        held: String,
    }

    let mut out = Vec::new();
    let mut open: Option<Open> = None;
    for (index, raw) in body.lines().enumerate() {
        let trimmed = raw.trim();
        if let Some(state) = open.as_mut() {
            if trimmed.starts_with(state.marker.as_str()) {
                if let Some(state) = open.take()
                    && state.wanted
                {
                    out.push((state.line, state.held));
                }
                continue;
            }
            if state.wanted {
                state.held.push_str(raw);
                state.held.push('\n');
            }
        } else if let Some(marker) = opening_fence(trimmed) {
            let info = trimmed.get(marker.len()..).unwrap_or_default().trim();
            open = Some(Open {
                wanted: info == language,
                marker,
                line: index.saturating_add(1),
                held: String::new(),
            });
        }
    }
    out
}

/// A `::: kind` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// One screen, one step of a deck.
    Card,
    /// A diagram with a caption, on its own screen.
    Figure,
    /// One practice item, reviewed as a flashcard.
    Exercise,
    /// An exercise's flashcard back.
    Answer,
    /// An exercise's worked answer.
    Solution,
    /// A result the learner looks up, declared in the deck that teaches it.
    Reference,
    /// A reference's bare statement, in LaTeX.
    Equation,
    /// What each symbol in an equation stands for.
    Legend,
    /// Why a reference holds.
    Derivation,
}

impl BlockKind {
    /// The word that opens this block.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Card => "card",
            Self::Figure => "figure",
            Self::Exercise => "exercise",
            Self::Answer => "answer",
            Self::Solution => "solution",
            Self::Reference => "reference",
            Self::Equation => "equation",
            Self::Legend => "legend",
            Self::Derivation => "derivation",
        }
    }

    /// Parse the word that opens a block.
    ///
    /// # Errors
    /// Returns [`FaultKind::UnknownBlock`] for a word the grammar does not hold.
    pub fn parse(word: &str) -> Result<Self, FaultKind> {
        Ok(match word {
            "card" => Self::Card,
            "figure" => Self::Figure,
            "exercise" => Self::Exercise,
            "answer" => Self::Answer,
            "solution" => Self::Solution,
            "reference" => Self::Reference,
            "equation" => Self::Equation,
            "legend" => Self::Legend,
            "derivation" => Self::Derivation,
            other => {
                return Err(FaultKind::UnknownBlock {
                    word: other.to_owned(),
                });
            }
        })
    }

    /// Whether this block may sit at the top level of a file, rather than inside
    /// another block.
    #[must_use]
    pub const fn stands_alone(self) -> bool {
        matches!(
            self,
            Self::Card | Self::Figure | Self::Exercise | Self::Reference
        )
    }

    /// Whether `child` may sit directly inside this block.
    #[must_use]
    pub const fn holds(self, child: Self) -> bool {
        match self {
            Self::Exercise => matches!(child, Self::Answer | Self::Solution),
            Self::Reference => matches!(child, Self::Equation | Self::Legend | Self::Derivation),
            _ => false,
        }
    }
}

impl std::fmt::Display for BlockKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One parsed block: its kind, its optional id, its own text, and what it holds.
#[derive(Debug, Clone)]
pub struct Block {
    pub kind: BlockKind,
    pub id: Option<String>,
    pub body: String,
    pub children: Vec<Self>,
    /// The line the block opened on, counted from 1, for an error message.
    pub line: usize,
    /// The file line of each line of `body`, in order. A child block takes
    /// its own lines, so the body is not always one run of the file.
    pub body_lines: Vec<usize>,
}

impl Block {
    /// The first child of a kind, if any.
    #[must_use]
    pub fn child(&self, kind: BlockKind) -> Option<&Self> {
        self.children.iter().find(|c| c.kind == kind)
    }

    /// The file line of one line of `body`, counted from 1. The block's own
    /// line when the body has no such line.
    #[must_use]
    pub fn file_line(&self, body_line: usize) -> usize {
        file_line(&self.body_lines, body_line).unwrap_or(self.line)
    }
}

/// The file line of one line of a text, from the lines it was read from.
#[must_use]
pub fn file_line(lines: &[usize], line: usize) -> Option<usize> {
    lines.get(line.checked_sub(1)?).copied()
}

/// How many lines of a body the `# Title` takes, with the blank lines above
/// it. The prose [`split_title`] returns starts after them.
#[must_use]
pub fn title_lines(body: &str) -> usize {
    let mut skipped = 0_usize;
    for line in body.lines() {
        skipped = skipped.saturating_add(1);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return if trimmed.starts_with("# ") {
            skipped
        } else {
            0
        };
    }
    0
}

/// Split a body into its `# Title` line and the prose after it.
///
/// The title is the first line with text, when it opens with `# `. A reference
/// block names itself this way, as a checkpoint once did, because a title with
/// its own prose after it reads better as a heading than as a field.
#[must_use]
pub fn split_title(body: &str) -> (Option<String>, String) {
    let mut lines = body.lines();
    let mut skipped = 0_usize;
    for line in lines.by_ref() {
        skipped = skipped.saturating_add(1);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return trimmed.strip_prefix("# ").map_or_else(
            || (None, body.to_owned()),
            |title| {
                let rest: Vec<&str> = body.lines().skip(skipped).collect();
                (Some(title.trim().to_owned()), rest.join("\n"))
            },
        );
    }
    (None, body.to_owned())
}

/// A parsed file: its front matter, the text outside any block, and its blocks.
#[derive(Debug, Clone, Default)]
pub struct Document {
    pub front: String,
    pub text: String,
    /// The file line of each line of `text`, in order.
    pub text_lines: Vec<usize>,
    pub blocks: Vec<Block>,
}

/// Parse a file into front matter, loose text, and nested blocks.
///
/// # Errors
/// Returns the line and the fault for an unclosed block, a stray `:::`, or a
/// block word the grammar does not hold.
pub fn parse(src: &str) -> Result<Document, (usize, FaultKind)> {
    let (front, body, offset) = split_front_matter(src);

    let mut text = String::new();
    let mut text_lines: Vec<usize> = Vec::new();
    let mut roots: Vec<Block> = Vec::new();
    let mut stack: Vec<Block> = Vec::new();

    // A card may show the format's own syntax, and a `cetz` snippet may open a
    // line with `:::`. Inside a fence that is content, never a block marker.
    let mut fence: Option<String> = None;
    for (index, raw) in body.lines().enumerate() {
        let line = offset.saturating_add(index).saturating_add(1);
        let trimmed = raw.trim();
        if let Some(open) = fence.as_deref() {
            if trimmed.starts_with(open) {
                fence = None;
            }
            push_line(&mut stack, (&mut text, &mut text_lines), raw, line);
            continue;
        }
        if let Some(ticks) = opening_fence(trimmed) {
            fence = Some(ticks);
            push_line(&mut stack, (&mut text, &mut text_lines), raw, line);
            continue;
        }
        if let Some((word, id)) = parse_open(trimmed) {
            let kind = BlockKind::parse(word).map_err(|fault| (line, fault))?;
            if stack.len() >= MAX_NESTING {
                return Err((line, FaultKind::NestedTooDeep(kind.to_string())));
            }
            stack.push(Block {
                kind,
                id,
                body: String::new(),
                children: Vec::new(),
                line,
                body_lines: Vec::new(),
            });
        } else if trimmed == ":::" {
            let done = stack.pop().ok_or((line, FaultKind::UnopenedBlock))?;
            match stack.last_mut() {
                Some(parent) => parent.children.push(done),
                None => roots.push(done),
            }
        } else {
            push_line(&mut stack, (&mut text, &mut text_lines), raw, line);
        }
    }

    if let Some(open) = stack.last() {
        return Err((open.line, FaultKind::UnclosedBlock(open.kind.to_string())));
    }
    Ok(Document {
        front,
        text,
        text_lines,
        blocks: roots,
    })
}

/// The run of backticks or tildes that opens a fence, if this line opens one.
fn opening_fence(line: &str) -> Option<String> {
    for marker in ['`', '~'] {
        let ticks: String = line.chars().take_while(|c| *c == marker).collect();
        if ticks.len() >= 3 {
            return Some(ticks);
        }
    }
    None
}

/// Put a line where it belongs: in the open block, else in the loose text.
fn push_line(stack: &mut [Block], loose: (&mut String, &mut Vec<usize>), raw: &str, line: usize) {
    let (sink, lines) = match stack.last_mut() {
        Some(open) => (&mut open.body, &mut open.body_lines),
        None => loose,
    };
    sink.push_str(raw);
    sink.push('\n');
    lines.push(line);
}

/// Split leading `---` front matter from the body, and say which line the body
/// starts on (counted from 0), so a fault inside it points at the real line.
fn split_front_matter(src: &str) -> (String, String, usize) {
    let mut lines = src.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (String::new(), src.to_owned(), 0);
    }
    let mut front = String::new();
    let mut at = 1_usize;
    for line in lines.by_ref() {
        at = at.saturating_add(1);
        if line.trim() == "---" {
            let body: Vec<&str> = src.lines().skip(at).collect();
            return (front, body.join("\n"), at);
        }
        front.push_str(line);
        front.push('\n');
    }
    // No closing `---`: treat the whole file as body, so the front-matter fault
    // is reported by the caller rather than silently swallowing the content.
    (String::new(), src.to_owned(), 0)
}

/// Read a `::: kind [id]` opening line.
fn parse_open(line: &str) -> Option<(&str, Option<String>)> {
    let rest = line.strip_prefix(":::")?.trim();
    if rest.is_empty() {
        return None;
    }
    let mut words = rest.split_whitespace();
    let kind = words.next()?;
    Some((kind, words.next().map(str::to_owned)))
}

#[cfg(test)]
mod tests {
    use super::{BlockKind, parse};

    #[test]
    fn front_matter_splits_from_the_body() {
        let doc = parse("---\ntitle: Ohm's law\n---\n\n::: card\nHello\n:::\n").expect("parses");
        assert_eq!(doc.front.trim(), "title: Ohm's law");
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.blocks[0].kind, BlockKind::Card);
        assert_eq!(doc.blocks[0].body.trim(), "Hello");
    }

    #[test]
    fn a_file_without_front_matter_is_all_body() {
        let doc = parse("::: card\nHello\n:::\n").expect("parses");
        assert!(doc.front.is_empty());
        assert_eq!(doc.blocks.len(), 1);
    }

    #[test]
    fn blocks_nest_and_carry_ids() {
        let doc = parse("::: exercise q1\nPrompt\n::: answer\nBack\n:::\n:::\n").expect("parses");
        let ex = &doc.blocks[0];
        assert_eq!(ex.kind, BlockKind::Exercise);
        assert_eq!(ex.id.as_deref(), Some("q1"));
        assert_eq!(
            ex.child(BlockKind::Answer).map(|a| a.body.trim()),
            Some("Back")
        );
    }

    #[test]
    fn a_fenced_diagram_stays_in_the_body() {
        let doc = parse("::: card\n```cetz\ncircle((0,0))\n```\n:::\n").expect("parses");
        assert!(doc.blocks[0].body.contains("```cetz"));
    }

    #[test]
    fn an_unclosed_block_reports_the_line_it_opened_on() {
        let (line, fault) = parse("\n\n::: card\nHello\n").expect_err("must fail");
        assert_eq!(line, 3);
        assert!(matches!(fault, crate::error::FaultKind::UnclosedBlock(_)));
    }

    #[test]
    fn a_stray_close_reports_its_own_line() {
        let (line, fault) = parse("::: card\n:::\n:::\n").expect_err("must fail");
        assert_eq!(line, 3);
        assert!(matches!(fault, crate::error::FaultKind::UnopenedBlock));
    }

    #[test]
    fn an_unknown_block_names_itself() {
        let (_, fault) = parse("::: lesson\n:::\n").expect_err("must fail");
        match fault {
            crate::error::FaultKind::UnknownBlock { word } => assert_eq!(word, "lesson"),
            other => panic!("wrong fault: {other}"),
        }
    }

    #[test]
    fn a_line_inside_front_matter_counts_from_the_real_file() {
        let (line, _) = parse("---\ntitle: X\n---\n::: card\n").expect_err("must fail");
        assert_eq!(line, 4);
    }

    /// A card that shows the format's own syntax must not end early.
    #[test]
    fn a_marker_inside_a_fence_is_content() {
        let doc = parse("::: card\n```\n::: card\n:::\n```\nafter\n:::\n").expect("parses");
        assert_eq!(doc.blocks.len(), 1, "one card, not three");
        assert!(doc.blocks[0].body.contains("after"));
        assert!(doc.blocks[0].body.contains("::: card"));
    }

    /// A `cetz` snippet may open a line with a colon run.
    #[test]
    fn a_diagram_may_hold_a_marker() {
        let doc =
            parse("::: figure f1\n```cetz\n:::whatever\n```\nCaption\n:::\n").expect("parses");
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.blocks[0].kind, BlockKind::Figure);
        assert!(doc.blocks[0].body.contains("Caption"));
    }

    /// A longer fence closes only on its own length.
    #[test]
    fn a_nested_fence_closes_on_its_own_marker() {
        let doc = parse("::: card\n````\n```\n:::\n```\n````\nx\n:::\n").expect("parses");
        assert_eq!(doc.blocks.len(), 1);
        assert!(doc.blocks[0].body.contains('x'));
    }

    /// A `plot` fence is found by its language, and a fence of another
    /// language is left alone.
    #[test]
    fn a_fence_is_read_by_its_language() {
        let body = "Prose\n\n```plot\nx: 1\n```\n\n```\nnot a plot\n```\n";
        let found = super::fences(body, super::PLOT);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, 3, "the line the fence opened on");
        assert_eq!(found[0].1, "x: 1\n");
    }

    #[test]
    fn a_title_line_splits_from_its_prose() {
        let (title, rest) = super::split_title("\n# Ohm's law\nThe current is…\n");
        assert_eq!(title.as_deref(), Some("Ohm's law"));
        assert_eq!(rest.trim(), "The current is…");
        let (none, all) = super::split_title("The current is…\n# Not a title\n");
        assert!(none.is_none());
        assert!(all.contains("The current is"));
    }

    /// A block opens at most one level down. A file that nests deeper is
    /// refused at the line that goes too deep, so no walk over the blocks
    /// recurses without a bound.
    #[test]
    fn a_block_two_levels_down_is_refused() {
        let (line, fault) =
            parse("::: exercise\n::: answer\n::: card\nx\n:::\n:::\n:::\n").expect_err("too deep");
        assert_eq!(line, 3);
        assert!(matches!(fault, crate::error::FaultKind::NestedTooDeep(_)));
        // The audit's probe: thousands of levels, which used to overflow the
        // stack of every later walk.
        let deep = "::: card\n".repeat(50_000);
        let (line, _) = parse(&deep).expect_err("too deep");
        assert_eq!(line, 3);
    }

    #[test]
    fn nesting_rules_hold() {
        assert!(BlockKind::Exercise.holds(BlockKind::Answer));
        assert!(BlockKind::Exercise.holds(BlockKind::Solution));
        assert!(!BlockKind::Card.holds(BlockKind::Answer));
        assert!(!BlockKind::Answer.stands_alone());
        assert!(BlockKind::Card.stands_alone());
        // A reference holds its equation, its legend and its derivation, and
        // none of the three stands alone any more.
        assert!(BlockKind::Reference.stands_alone());
        assert!(BlockKind::Reference.holds(BlockKind::Equation));
        assert!(BlockKind::Reference.holds(BlockKind::Legend));
        assert!(BlockKind::Reference.holds(BlockKind::Derivation));
        assert!(!BlockKind::Derivation.stands_alone());
        assert!(!BlockKind::Equation.stands_alone());
    }
}
