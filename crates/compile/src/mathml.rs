//! The MathML allowlist: **re-emit the renderer's math, never forward it**.
//!
//! The LaTeX renderer writes the text of `\text{…}` and `\operatorname{…}` into
//! its MathML as it finds it, with no escaping. So `$\text{<img onerror=…>}$`
//! comes out of it as a live `<img>`, and its MathML is author markup like any
//! other. This module is the boundary for it, built as [`crate::svg`] is: it
//! **reads** the renderer's string and **writes a new one** from the elements
//! and the attributes it knows, escaping every text and every value itself.
//!
//! The lists hold exactly what the renderer writes for valid LaTeX, and
//! nothing that can fetch, script, style or link: no `href`, no `style`, no
//! `class`, no `id`, no `maction`, no `annotation-xml`. A `class` the renderer
//! writes (`menv-*`) and a `style` (a `\color`, the negative margin of `\!`)
//! do not survive; the app styles neither. An element not on the list, such as
//! an `<img>` or a `<style>` from inside a `\text{…}`, **goes with its whole
//! subtree**, so its text cannot come back either. Everything after the root
//! `</math>` goes too.

use std::fmt::Write as _;

use quick_xml::Reader;
use quick_xml::events::Event;
use quick_xml::events::attributes::Attribute;

use crate::render::escape_attr;

/// Elements that survive: the ones the renderer writes for valid LaTeX.
const ELEMENTS: [&str; 19] = [
    "math",
    "mrow",
    "mi",
    "mn",
    "mo",
    "mtext",
    "mspace",
    "msup",
    "msub",
    "msubsup",
    "mfrac",
    "msqrt",
    "mroot",
    "mover",
    "munder",
    "munderover",
    "mtable",
    "mtr",
    "mtd",
];

/// The token elements, the only ones whose text is kept. Everywhere else the
/// renderer writes no text, so text there is dropped.
const TEXTUAL: [&str; 4] = ["mi", "mn", "mo", "mtext"];

/// How an attribute's value is checked. Each kind is a closed grammar, so no
/// value can carry a URL, a scheme or CSS.
#[derive(Clone, Copy)]
enum Value {
    /// `block` or `inline`.
    Display,
    /// `true` or `false`.
    Bool,
    /// A small signed integer.
    Level,
    /// A lowercase word, such as `normal`.
    Word,
    /// A number with an optional unit, such as `0.1667em` or `-3pt`.
    Length,
}

/// Every attribute that survives, and how its value is checked.
const ATTRIBUTES: [(&str, Value); 13] = [
    ("display", Value::Display),
    ("displaystyle", Value::Bool),
    ("scriptlevel", Value::Level),
    ("mathvariant", Value::Word),
    ("stretchy", Value::Bool),
    ("symmetric", Value::Bool),
    ("largeop", Value::Bool),
    ("movablelimits", Value::Bool),
    ("minsize", Value::Length),
    ("maxsize", Value::Length),
    ("linethickness", Value::Length),
    ("width", Value::Length),
    ("height", Value::Length),
];

/// The units a length may carry.
const UNITS: [&str; 10] = ["em", "ex", "mm", "cm", "in", "pt", "pc", "px", "mu", ""];

/// How deep math may nest. Real math is far shallower, and a page past this
/// is slow for the browser to lay out.
pub const MAX_DEPTH: usize = 256;

/// Re-emit the renderer's MathML as markup this app wrote.
///
/// # Errors
/// Returns what is wrong for the author to read: math that nests past
/// [`MAX_DEPTH`], or a string that will not read as markup at all.
pub fn reemit(source: &str) -> Result<String, String> {
    let prepared = well_formed(source);
    let mut reader = Reader::from_str(&prepared);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = false;

    let mut out = String::with_capacity(source.len());
    // The elements whose start tag we wrote. A name we did not write never
    // reaches this stack, so an end tag closes the element we opened.
    let mut open: Vec<&'static str> = Vec::new();
    // While skipping an element we do not know, how deep inside it we are.
    let mut skipping: usize = 0;
    let mut saw_root = false;

    loop {
        let event = reader
            .read_event()
            .map_err(|err| format!("its markup will not read: {err}"))?;
        match event {
            Event::Eof => break,
            Event::Start(ref tag) | Event::Empty(ref tag) => {
                let name = local_name(tag.name().as_ref());
                let holds_children =
                    matches!(event, Event::Start(_)) && !VOID.contains(&name.as_str());
                if skipping > 0 {
                    if holds_children {
                        skipping = skipping.saturating_add(1);
                    }
                    continue;
                }
                // The root is `math`, and `math` is only the root.
                let is_math = name == "math";
                let in_place = if saw_root { !is_math } else { is_math };
                let Some(known) = known(&name).filter(|_| in_place) else {
                    // The element goes, and everything under it goes with it.
                    if holds_children {
                        skipping = 1;
                    }
                    continue;
                };
                saw_root = true;
                if open.len() >= MAX_DEPTH {
                    return Err(format!(
                        "it nests deeper than {MAX_DEPTH} levels: simplify it"
                    ));
                }
                let _ = write!(out, "<{known}");
                for attribute in tag.attributes().flatten() {
                    write_attribute(&mut out, &attribute);
                }
                if holds_children {
                    out.push('>');
                    open.push(known);
                } else {
                    out.push_str(" />");
                    if open.is_empty() {
                        break;
                    }
                }
            }
            Event::End(_) => {
                if skipping > 0 {
                    skipping = skipping.saturating_sub(1);
                    continue;
                }
                if let Some(name) = open.pop() {
                    let _ = write!(out, "</{name}>");
                    // The root is closed. Nothing after it is math.
                    if open.is_empty() {
                        break;
                    }
                }
            }
            Event::Text(text) => {
                if skipping == 0 && in_text(&open) {
                    out.push_str(&escape_text(&text.xml10_content()));
                }
            }
            // An entity arrives as its own event. A numeric reference, the
            // five XML names and `nbsp` (which the renderer writes for the
            // space at either end of a `\text{…}`) resolve to a character,
            // which is written back escaped. Any other name is dropped.
            Event::GeneralRef(entity) => {
                if skipping == 0
                    && in_text(&open)
                    && let Some(character) = resolve_entity(&entity)
                {
                    out.push_str(&escape_text(&character.to_string()));
                }
            }
            // A comment, a processing instruction, a doctype and a CDATA
            // section carry no math, so none of them is written.
            _ => {}
        }
    }

    // The renderer closes every element it opens; this closes the ones a
    // stray end tag inside a `\text{…}` left open.
    while let Some(name) = open.pop() {
        let _ = write!(out, "</{name}>");
    }
    if !saw_root {
        return Err("it holds no <math> element".to_owned());
    }
    Ok(out)
}

/// Make the renderer's string well-formed, so the reader reads every byte.
///
/// The renderer writes a relation as its raw character (`<mo><</mo>`), the text
/// of a `\text{…}` as it stands (`a & b`), and one attribute with no space
/// before it (`stretchy="true"minsize=…`). A browser reads all three, and an
/// XML reader reads none of them. So a `<` opens a tag only before a letter, or
/// a `/` and a letter, and only when its `>` comes before another `<`. Every
/// other `<`, and every `&` that starts no reference, becomes text. This
/// decides only how the string is read: what is written comes from [`reemit`]
/// alone. Each byte is read at most twice, so the time is linear.
fn well_formed(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    // The tag being read: where it starts in `source`, and the tag as it will
    // be written. It is held until its `>`, because a `<` before that makes
    // it text after all.
    let mut tag: Option<(usize, String)> = None;
    let mut quote: Option<char> = None;
    let mut closed_value = false;
    for (at, c) in source.char_indices() {
        let next = source
            .get(at.saturating_add(c.len_utf8())..)
            .unwrap_or_default();
        if let Some((start, held)) = tag.as_mut() {
            if let Some(open) = quote {
                if c == open {
                    quote = None;
                    closed_value = true;
                    held.push(c);
                } else {
                    push_text(held, c, next);
                }
                continue;
            }
            if c == '<' {
                // Not a tag: the held run is text, and this `<` starts afresh.
                for (offset, held_c) in source.get(*start..at).unwrap_or_default().char_indices() {
                    let rest = source
                        .get(
                            start
                                .saturating_add(offset)
                                .saturating_add(held_c.len_utf8())..,
                        )
                        .unwrap_or_default();
                    push_text(&mut out, held_c, rest);
                }
                tag = None;
                closed_value = false;
            } else {
                if closed_value && !(c.is_whitespace() || c == '/' || c == '>') {
                    held.push(' ');
                }
                closed_value = false;
                held.push(c);
                match c {
                    '"' | '\'' => quote = Some(c),
                    '>' => {
                        out.push_str(held);
                        tag = None;
                    }
                    _ => {}
                }
                continue;
            }
        }
        if c == '<' && opens_tag(next) {
            tag = Some((at, String::from('<')));
        } else {
            push_text(&mut out, c, next);
        }
    }
    // A tag that never closed is text.
    if let Some((start, _)) = tag {
        let rest = source.get(start..).unwrap_or_default();
        for (offset, c) in rest.char_indices() {
            let next = rest
                .get(offset.saturating_add(c.len_utf8())..)
                .unwrap_or_default();
            push_text(&mut out, c, next);
        }
    }
    out
}

/// Push one character of text, escaped as markup needs it. `next` is the text
/// after it, to tell a reference from a bare `&`.
fn push_text(out: &mut String, c: char, next: &str) {
    match c {
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '&' if !reference_at(next) => out.push_str("&amp;"),
        other => out.push(other),
    }
}

/// Whether the text after a `<` makes it a tag: a letter, or `/` and a letter.
fn opens_tag(next: &str) -> bool {
    let mut chars = next.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => true,
        Some('/') => chars.next().is_some_and(|c| c.is_ascii_alphabetic()),
        _ => false,
    }
}

/// Whether the text after a `&` is a reference: a name or a number of 32
/// characters at most, then `;`. The look stops there, so it costs nothing.
fn reference_at(next: &str) -> bool {
    let body: String = next.chars().take(34).take_while(|c| *c != ';').collect();
    if body.len() >= 34
        || !next
            .get(body.len()..)
            .is_some_and(|rest| rest.starts_with(';'))
    {
        return false;
    }
    let number = body.strip_prefix('#');
    let digits = number.map_or(body.as_str(), |rest| {
        rest.strip_prefix(['x', 'X']).unwrap_or(rest)
    });
    !digits.is_empty() && digits.len() <= 32 && digits.chars().all(|c| c.is_ascii_alphanumeric())
}

/// The HTML elements that hold no children. One of them, dropped, takes only
/// itself: `<img>` has no end tag, so its "subtree" would be the rest of the
/// math.
const VOID: [&str; 13] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// The name this module writes for an element, where the element is known.
fn known(lower: &str) -> Option<&'static str> {
    ELEMENTS.iter().find(|known| **known == lower).copied()
}

/// Whether the element being read is a token, whose text is kept.
fn in_text(open: &[&str]) -> bool {
    open.last().is_some_and(|name| TEXTUAL.contains(name))
}

/// One entity as the character it stands for, where it stands for one.
fn resolve_entity(entity: &quick_xml::events::BytesRef<'_>) -> Option<char> {
    if let Ok(Some(character)) = entity.resolve_char_ref() {
        return Some(character);
    }
    match entity.clone().into_inner().as_ref() {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => None,
    }
}

/// Write one attribute, if it is on the list and its value passes.
fn write_attribute(out: &mut String, attribute: &Attribute<'_>) {
    let name = local_name(attribute.key.as_ref());
    let Some((written, kind)) = ATTRIBUTES.iter().find(|(known, _)| *known == name) else {
        return;
    };
    let Ok(raw) = attribute.normalized_value(quick_xml::XmlVersion::Explicit1_0) else {
        return;
    };
    let value = raw.trim();
    if passes(*kind, value) {
        let _ = write!(out, " {written}=\"{}\"", escape_attr(value));
    }
}

/// Whether a value is what its kind allows.
fn passes(kind: Value, value: &str) -> bool {
    match kind {
        Value::Display => matches!(value, "block" | "inline"),
        Value::Bool => matches!(value, "true" | "false"),
        Value::Level => {
            let digits = value.strip_prefix(['-', '+']).unwrap_or(value);
            (1..=2).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_digit())
        }
        Value::Word => {
            (1..=32).contains(&value.len())
                && value.chars().all(|c| c.is_ascii_lowercase() || c == '-')
        }
        Value::Length => {
            let number = value.strip_prefix('-').unwrap_or(value);
            let split = number
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(number.len());
            let (digits, unit) = number.split_at(split);
            (1..=24).contains(&digits.len())
                && digits.chars().any(|c| c.is_ascii_digit())
                && digits.matches('.').count() <= 1
                && UNITS.contains(&unit)
        }
    }
}

/// An element or attribute name, without its namespace prefix, lowercased.
fn local_name(raw: &str) -> String {
    raw.rsplit(':').next().unwrap_or(raw).to_ascii_lowercase()
}

/// Escape text this module writes. The no-break space is written as the
/// renderer writes it, `&nbsp;`, so plain math leaves here as it arrived.
fn escape_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::reemit;

    /// Plain math leaves as it arrived.
    #[test]
    fn plain_math_is_unchanged() {
        for math in [
            "<math display=\"inline\"><mi>x</mi></math>",
            "<math display=\"block\"><mfrac><mrow><mi>a</mi></mrow><mrow><mi>b</mi></mrow></mfrac></math>",
            "<math display=\"inline\"><mi>sin</mi><mo>\u{2061}</mo><mspace width=\"0.1667em\" /><mi>x</mi></math>",
            "<math display=\"inline\"><mtext>if&nbsp;</mtext><mi>x</mi></math>",
            "<math display=\"inline\"><msubsup><mo movablelimits=\"false\">∑</mo><mi>i</mi><mi>n</mi></msubsup></math>",
        ] {
            assert_eq!(reemit(math).expect("re-emits"), math);
        }
    }

    /// A relation written as its raw character reads as text, and leaves
    /// escaped.
    #[test]
    fn a_raw_relation_is_escaped() {
        let out =
            reemit("<math display=\"inline\"><mi>x</mi><mo><</mo><mn>0</mn><mi>&</mi></math>")
                .expect("re-emits");
        assert_eq!(
            out,
            "<math display=\"inline\"><mi>x</mi><mo>&lt;</mo><mn>0</mn><mi>&amp;</mi></math>"
        );
    }

    /// The renderer writes one attribute with no space before it. It is read
    /// all the same.
    #[test]
    fn a_missing_space_between_attributes_is_read() {
        let out = reemit(
            "<math display=\"inline\"><mo symmetric=\"true\" stretchy=\"true\"minsize=\"1.2em\" maxsize=\"1.2em\">(</mo></math>",
        )
        .expect("re-emits");
        assert_eq!(
            out,
            "<math display=\"inline\"><mo symmetric=\"true\" stretchy=\"true\" minsize=\"1.2em\" maxsize=\"1.2em\">(</mo></math>"
        );
    }

    /// Markup inside a `\text{…}` goes with its subtree, and so does anything
    /// after the root closes.
    #[test]
    fn markup_from_text_never_reaches_the_page() {
        for (math, gone) in [
            (
                "<math display=\"inline\"><mtext><img src=x onerror=alert(1)></mtext></math>",
                "<img",
            ),
            (
                "<math display=\"inline\"><mtext><style>body{}</style></mtext></math>",
                "<style",
            ),
            (
                "<math display=\"inline\"><mtext></mtext></math><script>alert(1)</script>",
                "<script",
            ),
            (
                "<math display=\"inline\"><mtext></mtext></math><mtext>after</mtext>",
                "after",
            ),
            (
                "<math display=\"inline\"><mtext></mtext><math><mi>x</mi></math></math>",
                "<mi>",
            ),
        ] {
            let out = reemit(math).expect("re-emits");
            assert!(!out.contains(gone), "{math}: {out}");
        }
    }

    /// Only the listed attributes survive, and only with a value that passes.
    #[test]
    fn an_attribute_off_the_list_is_dropped() {
        let out = reemit(
            "<math display=\"inline\" href=\"https://evil.example\"><mtext class=\"x\" style=\"color:red\" id=\"a\" onclick=\"alert(1)\" mathvariant=\"url(x)\" width=\"javascript:1\">t</mtext></math>",
        )
        .expect("re-emits");
        assert_eq!(out, "<math display=\"inline\"><mtext>t</mtext></math>");
    }

    /// Math past the depth cap is refused rather than drawn.
    #[test]
    fn math_past_the_depth_cap_is_refused() {
        let deep = format!(
            "<math>{}<mi>x</mi>{}</math>",
            "<mrow>".repeat(super::MAX_DEPTH),
            "</mrow>".repeat(super::MAX_DEPTH)
        );
        assert!(reemit(&deep).is_err());
    }
}
