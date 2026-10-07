//! Render one Markdown body to self-contained HTML.
//!
//! Prose becomes real DOM text, `$math$` becomes native MathML, an asset is
//! inlined, and the two link forms become their marks. The structure of a file
//! (its front matter and its `:::` blocks) is the `format` crate's business, so
//! this module renders and never parses structure.
//!
//! **Raw HTML is off.** The app generates every byte of markup a learner sees,
//! so an author's `<script>` arrives here as an event and leaves as escaped
//! text. The two exceptions are an author's SVG and the MathML the LaTeX
//! renderer writes, which are re-emitted through [`crate::svg`] and
//! [`crate::mathml`], never forwarded.

use std::fmt::Write as _;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};
use pulldown_latex::config::{DisplayMode, RenderConfig};
use pulldown_latex::{Parser as LatexParser, Storage, push_mathml};
use stemin_format::FaultKind;
use stemin_format::block::PLOT;
use stemin_format::plot::Plot;

/// What a body needs from the compiler around it: a reference to resolve, a
/// figure to number, and an asset to inline.
///
/// Three closures rather than three trait methods, because each one closes over
/// a different part of the compile (the domain, the deck's figures, the
/// repository's files) and none of them outlives one body.
pub struct Ctx<'a> {
    /// A `reference:` link's target to a global id.
    pub reference: &'a mut dyn FnMut(&str) -> Result<String, FaultKind>,
    /// A `figure:` link's target to the anchor that names its number.
    pub figure: &'a mut dyn FnMut(&str) -> Result<String, FaultKind>,
    /// An `![alt](assets/…)` reference to the markup that inlines it.
    pub asset: &'a mut dyn FnMut(&str, &str) -> Result<String, FaultKind>,
}

/// One `reference:` link a body makes.
#[derive(Debug, Clone)]
pub struct Link {
    /// The global id it resolves to. Whether a reference is there is checked
    /// once every domain is compiled, because a link may point into another.
    pub global: String,
    /// The link as the author wrote it, `reference:ohms-law`.
    pub written: String,
    /// The line of the body it sits on, counted from 1.
    pub line: usize,
}

/// One thing wrong with a body, at a line of it counted from 1.
#[derive(Debug)]
pub struct Failure {
    pub line: usize,
    pub kind: FaultKind,
}

/// The output of rendering one body: its HTML, the references it links, the
/// plots it declares, and everything wrong with it.
#[derive(Debug, Default)]
pub struct Rendered {
    pub html: String,
    /// The references this body links, in order.
    pub references: Vec<Link>,
    /// Every `plot` block, with the line it opens on. Only a card or a figure
    /// draws one, and only one; the compiler holds the body to that.
    pub plots: Vec<(usize, Plot)>,
    /// Every fault, in order. The render carries on past each one, so one run
    /// names them all.
    pub failures: Vec<Failure>,
}

/// Render one Markdown body to HTML.
///
/// It never stops at a fault: math that will not parse, a reference or a
/// figure that does not resolve, an asset that is missing, or a `plot` block
/// that will not read is recorded in [`Rendered::failures`] with its line, and
/// the render goes on.
#[must_use]
pub fn render_body(md: &str, ctx: &mut Ctx<'_>) -> Rendered {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_MATH);
    let (events, ranges): (Vec<Event<'_>>, Vec<std::ops::Range<usize>>) =
        Parser::new_ext(md, options).into_offset_iter().unzip();
    // Where each line ends, found once. A line is then a binary search, so a
    // body of many events costs no more than its length.
    let breaks: Vec<usize> = md.match_indices('\n').map(|(at, _)| at).collect();
    let line_at = |index: usize| -> usize {
        let start = ranges.get(index).map_or(0, |range| range.start);
        breaks.partition_point(|at| *at < start).saturating_add(1)
    };

    let mut out: Vec<Event<'_>> = Vec::with_capacity(events.len());
    let mut rendered = Rendered::default();
    let mut at = 0_usize;

    while let Some(event) = events.get(at) {
        let line = line_at(at);
        at = at.saturating_add(1);
        let mut failed: Option<FaultKind> = None;
        match event {
            Event::InlineMath(latex) | Event::DisplayMath(latex) => {
                let display = matches!(event, Event::DisplayMath(_));
                match latex_to_mathml(latex, display) {
                    Ok(mathml) => out.push(Event::InlineHtml(mathml.into())),
                    Err(why) => failed = Some(FaultKind::Render(why)),
                }
            }
            // The one fenced language. It is not drawn here: the app draws it
            // live and re-evaluates it as the learner drags a slider, so the
            // card carries the spec and the block leaves no markup behind.
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if info.trim() == PLOT => {
                let body = take_text(&events, &mut at, TagEnd::CodeBlock);
                match stemin_format::plot::parse(&body) {
                    Ok(plot) => rendered.plots.push((line, plot)),
                    Err(why) => failed = Some(FaultKind::BadPlot(why)),
                }
            }
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                if let Some(target) = dest_url.strip_prefix("reference:") {
                    let label = take_text(&events, &mut at, TagEnd::Link);
                    match (ctx.reference)(target) {
                        Ok(global) => {
                            out.push(Event::InlineHtml(
                                format!(
                                    "<a class=\"reference-link\" data-reference=\"{}\" title=\"{}\" aria-label=\"{}\">∎</a>",
                                    escape_attr(&global),
                                    escape_attr(label.trim()),
                                    escape_attr(label.trim()),
                                )
                                .into(),
                            ));
                            rendered.references.push(Link {
                                global,
                                written: dest_url.to_string(),
                                line,
                            });
                        }
                        Err(kind) => failed = Some(kind),
                    }
                } else if let Some(target) = dest_url.strip_prefix("figure:") {
                    let _ = take_text(&events, &mut at, TagEnd::Link);
                    match (ctx.figure)(target) {
                        Ok(anchor) => out.push(Event::InlineHtml(anchor.into())),
                        Err(kind) => failed = Some(kind),
                    }
                } else if safe_href(dest_url) {
                    // The start tag is this app's own, so it carries `rel`: the
                    // page it opens gets no handle on this one, and no
                    // referrer. The label's events follow, and the link's own
                    // end event closes it.
                    let mut open = format!(
                        "<a href=\"{}\" rel=\"noopener noreferrer\"",
                        escape_attr(dest_url.trim())
                    );
                    if !title.is_empty() {
                        let _ = write!(open, " title=\"{}\"", escape_attr(title));
                    }
                    open.push('>');
                    out.push(Event::InlineHtml(open.into()));
                } else {
                    // A link that could run code, or fetch something a card
                    // should not, reads as its own text and goes nowhere.
                    let label = take_text(&events, &mut at, TagEnd::Link);
                    out.push(Event::Text(label.into()));
                }
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                let alt = take_text(&events, &mut at, TagEnd::Image);
                match (ctx.asset)(dest_url, alt.trim()) {
                    Ok(markup) => out.push(Event::InlineHtml(markup.into())),
                    Err(kind) => failed = Some(kind),
                }
            }
            // Raw HTML an author wrote is content, not markup. It reads as the
            // text it is, so no author byte becomes markup.
            Event::Html(raw) | Event::InlineHtml(raw) => out.push(Event::Text(raw.clone())),
            other => out.push(other.clone()),
        }
        if let Some(kind) = failed {
            rendered.failures.push(Failure { line, kind });
        }
    }

    let mut html = String::new();
    html::push_html(&mut html, out.into_iter());
    html.trim().clone_into(&mut rendered.html);
    rendered
}

/// Whether a plain link may stay a link: the secure web, or mail. Everything
/// else (`http:`, `javascript:`, `data:`, a fragment, a relative path into
/// nothing) is refused by name rather than by blocklist, and reads as its text.
fn safe_href(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    ["https://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
}

/// Consume the events up to and including `end`, returning their text.
///
/// A link's text is its label, an image's is its alt, and a fenced block's is
/// its body. In each case the events between the two tags are replaced by
/// something this app writes, so they are read for their text and dropped.
fn take_text(events: &[Event<'_>], at: &mut usize, end: TagEnd) -> String {
    let mut text = String::new();
    while let Some(event) = events.get(*at) {
        *at = at.saturating_add(1);
        match event {
            Event::End(tag) if *tag == end => break,
            Event::Text(run) | Event::Code(run) | Event::InlineMath(run) => text.push_str(run),
            Event::SoftBreak | Event::HardBreak => text.push(' '),
            _ => {}
        }
    }
    text
}

/// Render a LaTeX fragment to inline MathML (e.g. a legend symbol).
///
/// # Errors
/// Returns the renderer's message if the LaTeX fails to parse.
pub fn inline_math(latex: &str) -> Result<String, String> {
    latex_to_mathml(latex, false)
}

/// Render a LaTeX fragment to display MathML (a reference's bare equation).
///
/// # Errors
/// Returns the renderer's message if the LaTeX fails to parse.
pub fn display_math(latex: &str) -> Result<String, String> {
    latex_to_mathml(latex, true)
}

/// Escape a plain-text string for safe inclusion in HTML text content (e.g. a
/// legend's meaning), so `&` and `<`/`>` cannot break or inject markup.
#[must_use]
pub fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Render one line of plain prose with inline `$…$` math (e.g. a legend's
/// meaning). The text between the spans is escaped, and each span becomes
/// inline MathML, so the line cannot inject markup.
///
/// # Errors
/// Returns a message if a `$` has no partner, or if a span fails to parse.
pub fn inline_prose(line: &str) -> Result<String, String> {
    if line.matches('$').count() % 2 == 1 {
        return Err(format!("a `$` has no closing `$` in \"{line}\""));
    }
    let mut out = String::new();
    for (index, part) in line.split('$').enumerate() {
        if index % 2 == 0 {
            out.push_str(&escape_text(part));
        } else {
            out.push_str(&inline_math(part)?);
        }
    }
    Ok(out)
}

/// Render a LaTeX fragment to a MathML string, re-emitted through the
/// allowlist in [`crate::mathml`].
///
/// The renderer does not fail on bad LaTeX: it writes an `<merror>` box into
/// the MathML and carries on, which a learner would see as a red box in a
/// card. So the parser's own errors are read as they pass, and any one of them
/// fails the fragment, in the checker and in the browser's import alike.
fn latex_to_mathml(latex: &str, display: bool) -> Result<String, String> {
    let storage = Storage::new();
    let mut errors: Vec<String> = Vec::new();
    let mut out = String::new();
    let config = RenderConfig {
        display_mode: if display {
            DisplayMode::Block
        } else {
            DisplayMode::Inline
        },
        ..Default::default()
    };
    let events = LatexParser::new(latex, &storage).inspect(|event| {
        // The parser's message carries a drawn box of context over several
        // lines. Its first line says what is wrong, and the fault already
        // names the LaTeX and its line.
        if let Err(error) = event {
            let message = error.to_string();
            errors.push(message.lines().next().unwrap_or_default().trim().to_owned());
        }
    });
    push_mathml(&mut out, events, config).map_err(|err| err.to_string())?;
    let shown = latex.trim();
    if let Some(first) = errors.first() {
        return Err(format!("the LaTeX `{shown}` will not render: {first}"));
    }
    // The renderer writes the text of `\text{…}` and `\operatorname{…}` as
    // markup, so its output is author markup too. It is re-emitted, never
    // forwarded.
    crate::mathml::reemit(&out).map_err(|why| format!("the LaTeX `{shown}` will not render: {why}"))
}

/// Escape a string for an HTML double-quoted attribute.
pub(crate) fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::{Ctx, render_body};
    use stemin_format::FaultKind;

    /// A context that resolves everything, for the rendering tests.
    fn ctx<'a>(
        reference: &'a mut dyn FnMut(&str) -> Result<String, FaultKind>,
        figure: &'a mut dyn FnMut(&str) -> Result<String, FaultKind>,
        asset: &'a mut dyn FnMut(&str, &str) -> Result<String, FaultKind>,
    ) -> Ctx<'a> {
        Ctx {
            reference,
            figure,
            asset,
        }
    }

    fn render(md: &str) -> super::Rendered {
        let mut reference = |target: &str| Ok(format!("dom/{target}"));
        let mut figure = |target: &str| Ok(format!("<a>{target}</a>"));
        let mut asset = |dest: &str, alt: &str| Ok(format!("<img src=\"{dest}\" alt=\"{alt}\"/>"));
        let out = render_body(md, &mut ctx(&mut reference, &mut figure, &mut asset));
        assert!(out.failures.is_empty(), "{:?}", out.failures);
        out
    }

    #[test]
    fn prose_and_inline_math_render() {
        let out = render("A *linear* equation $a x + b = 0$ has one root.");
        assert!(out.html.contains("<em>linear</em>"));
        assert!(out.html.contains("<math"), "{}", out.html);
    }

    #[test]
    fn display_math_carries_its_mode() {
        let out = render("$$ x = 3 $$");
        assert!(out.html.contains("display=\"block\""), "{}", out.html);
    }

    /// The reference link is the only inline link a card carries, and it reads
    /// as a quiet mark, never as the author's own words. The author's words
    /// name it for a screen reader and in its tooltip.
    #[test]
    fn a_reference_link_becomes_the_mark() {
        let out = render("See [Ohm's law](reference:ohms-law) for why.");
        assert!(out.html.contains("data-reference=\"dom/ohms-law\""));
        assert!(out.html.contains('∎'));
        assert!(!out.html.contains("Ohm&#39;s law</a>"));
        assert!(
            out.html.contains("aria-label=\"Ohm's law\""),
            "{}",
            out.html
        );
        let links: Vec<&str> = out.references.iter().map(|l| l.global.as_str()).collect();
        assert_eq!(links, vec!["dom/ohms-law"]);
    }

    #[test]
    fn a_plot_block_leaves_the_prose_and_becomes_a_spec() {
        let out = render(
            "Before.\n\n```plot\nx: { var: t, from: 0, to: 1 }\ny: { from: 0, to: 1 }\n\
             draw:\n  - curve: t\n```\n\nAfter.\n",
        );
        assert_eq!(out.plots.len(), 1);
        assert_eq!(out.plots[0].0, 3, "the line the fence opens on");
        assert!(out.html.contains("Before."));
        assert!(out.html.contains("After."));
        assert!(!out.html.contains("curve"), "{}", out.html);
    }

    /// No author byte becomes markup: a repository that holds a `<script>` in
    /// its Markdown renders it as text.
    #[test]
    fn raw_html_renders_as_text() {
        let out = render("<script>alert(1)</script>\n\nAnd <b>inline</b> too.");
        assert!(!out.html.contains("<script>"), "{}", out.html);
        assert!(out.html.contains("&lt;script&gt;"), "{}", out.html);
        assert!(!out.html.contains("<b>"), "{}", out.html);
    }

    /// A link that could run code is not a link: its text stays, its target
    /// goes. The CSP is the backstop, not the control.
    #[test]
    fn a_script_link_reads_as_text() {
        for md in [
            "[click](javascript:alert(1))",
            "<javascript:alert(1)>",
            "[x](data:text/html,<b>)",
            "[x](JaVaScRiPt:alert(1))",
            "[plain](http://example.com)",
            "[here](#top)",
        ] {
            let out = render(md);
            assert!(!out.html.contains("href"), "{md}: {}", out.html);
        }
        let out = render("[the book](https://example.com/book \"A \\\"title\\\"\")");
        assert!(
            out.html.contains(
                "<a href=\"https://example.com/book\" rel=\"noopener noreferrer\" \
                 title=\"A &quot;title&quot;\">the book</a>"
            ),
            "{}",
            out.html
        );
        let out = render("Write to [us](mailto:a@example.com).");
        assert!(
            out.html
                .contains("href=\"mailto:a@example.com\" rel=\"noopener noreferrer\""),
            "{}",
            out.html
        );
    }

    /// The C1 payloads of the audit: markup inside `\text{…}` or
    /// `\operatorname{…}` reaches the page as nothing, never as an element.
    #[test]
    fn markup_inside_latex_never_becomes_markup() {
        for latex in [
            r"\text{<img src=x onerror=alert(1)>}",
            r"\operatorname{<b>x</b>}",
            r"\text{<style>body{}</style>}",
            r#"\text{<meta http-equiv="refresh" content="0;url=https://evil.example/">}"#,
            r"\text{&lt;script&gt;}",
            r"\text{</mtext></math><img src=x onerror=alert(1)>}",
            r#"\text{</mtext><mtext style="color:red" onclick="alert(1)">}"#,
        ] {
            for md in [format!("${latex}$"), format!("$$ {latex} $$")] {
                let out = render(&md);
                for gone in [
                    "<img", "<style", "<meta", "<b>", "<script", "style=", "onclick",
                ] {
                    assert!(!out.html.contains(gone), "{md}: {}", out.html);
                }
            }
            for math in [super::inline_math(latex), super::display_math(latex)] {
                let math = math.expect("renders");
                assert!(!math.contains("<img") && !math.contains("<style"), "{math}");
                assert!(!math.contains("<meta") && !math.contains("<b>"), "{math}");
                assert!(!math.contains("<script"), "{math}");
            }
        }
    }

    /// Ordinary math is untouched by the allowlist: the renderer's own
    /// elements and attributes pass, and its text reads the same.
    #[test]
    fn ordinary_math_still_renders() {
        for (latex, expected) in [
            (
                r"\frac{a}{b}",
                "<math display=\"inline\"><mfrac><mrow><mi>a</mi></mrow><mrow><mi>b</mi></mrow></mfrac></math>",
            ),
            (
                r"\sqrt[3]{x}",
                "<math display=\"inline\"><mroot><mrow><mi>x</mi></mrow><mn>3</mn></mroot></math>",
            ),
            (
                r"\operatorname{sin} x",
                "<math display=\"inline\"><mi>sin</mi><mo>\u{2061}</mo><mspace width=\"0.1667em\" /><mi>x</mi></math>",
            ),
            (
                r"\text{if } x>0",
                "<math display=\"inline\"><mtext>if&nbsp;</mtext><mi>x</mi><mo>&gt;</mo><mn>0</mn></math>",
            ),
            (
                r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
                "<math display=\"inline\"><mrow><mo stretchy=\"true\">(</mo><mtable><mtr><mtd><mn>1</mn></mtd><mtd><mn>2</mn></mtd></mtr><mtr><mtd><mn>3</mn></mtd><mtd><mn>4</mn></mtd></mtr></mtable><mo stretchy=\"true\">)</mo></mrow></math>",
            ),
        ] {
            assert_eq!(super::inline_math(latex).expect("renders"), expected);
        }
    }

    #[test]
    fn prose_renders_its_math_and_escapes_its_text() {
        let out = super::inline_prose("a vector of $\\mathbb{R}^n$, x < y").expect("renders");
        assert!(out.starts_with("a vector of <math"), "{out}");
        assert!(out.ends_with("</math>, x &lt; y"), "{out}");
        assert_eq!(super::inline_prose("no math").expect("renders"), "no math");
        assert!(super::inline_prose("an open $x").is_err());
    }

    #[test]
    fn an_image_is_inlined_by_the_caller() {
        let out = render("![A trace](assets/scope.png)");
        assert!(
            out.html.contains("src=\"assets/scope.png\""),
            "{}",
            out.html
        );
        assert!(out.html.contains("alt=\"A trace\""), "{}", out.html);
    }

    #[test]
    fn a_broken_reference_fails_the_render_at_its_line() {
        let mut reference = |_: &str| Err(FaultKind::Render("no such reference".to_owned()));
        let mut figure = |target: &str| Ok(target.to_owned());
        let mut asset = |_: &str, _: &str| Ok(String::new());
        let out = render_body(
            "Line one.\n\nSee [x](reference:nope).",
            &mut ctx(&mut reference, &mut figure, &mut asset),
        );
        assert_eq!(out.failures.len(), 1);
        assert_eq!(out.failures[0].line, 3);
    }

    /// The renderer writes an `<merror>` box for bad LaTeX and carries on. A
    /// learner would see a red box, so the compile refuses it instead.
    #[test]
    fn bad_latex_fails_the_render() {
        let mut reference = |target: &str| Ok(target.to_owned());
        let mut figure = |target: &str| Ok(target.to_owned());
        let mut asset = |_: &str, _: &str| Ok(String::new());
        let out = render_body(
            "Fine $x$.\n\nBroken $\\frac{a}$.",
            &mut ctx(&mut reference, &mut figure, &mut asset),
        );
        assert_eq!(out.failures.len(), 1, "{:?}", out.failures);
        assert_eq!(out.failures[0].line, 3);
        let message = out.failures[0].kind.to_string();
        assert!(message.contains("will not render"), "{message}");
        assert!(!out.html.contains("merror"), "{}", out.html);
    }
}
