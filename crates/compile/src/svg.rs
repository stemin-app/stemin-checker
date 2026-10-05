//! The SVG allowlist: **re-emit an author's picture, never filter it**.
//!
//! An author's SVG is the one piece of untrusted markup in a repository. The
//! prose is Markdown with raw HTML off and the MathML is generated, so this
//! module is the security boundary.
//!
//! Filtering is the known-bad pattern, and its risk is mutation XSS: the
//! sanitizer reads the bytes one way, the browser reads them another, and
//! something slips between. So nothing here deletes anything from the author's
//! markup. It **reads** the file and **writes a new one** from the elements and
//! the attributes it knows, escaping every value itself. No author byte is ever
//! forwarded as markup, so there is nothing for a parser disagreement to
//! smuggle. An element this module does not know takes its whole subtree with
//! it, so a `<script>`'s body cannot come back as text.
//!
//! The allowlist is deliberately narrow, because a small allowlist is one that
//! can be audited: geometry, type, and the paint that themes it.
//!
//! The figure is one island in a page this app owns. So the markup stops at the
//! end of the root `<svg>`, an author's `class` does not survive, every `id`
//! takes a prefix from a hash of the file (and every `url(#…)` the same
//! prefix), and the root's `width` and `height` give way to its `viewBox`.
//! Nothing in a figure can reach out to style or to name anything outside it.

use std::fmt::Write as _;

use quick_xml::Reader;
use quick_xml::events::Event;
use quick_xml::events::attributes::Attribute;

/// Elements that survive. Geometry, type, and the two containers that group
/// them. Anything that can fetch, script, or lay out foreign content is absent
/// on purpose: `script`, `style`, `image`, `use`, `foreignObject`, `animate`.
const ELEMENTS: [&str; 18] = [
    "svg",
    "g",
    "defs",
    "title",
    "desc",
    "path",
    "line",
    "polyline",
    "polygon",
    "rect",
    "circle",
    "ellipse",
    "text",
    "tspan",
    "marker",
    "clippath",
    "lineargradient",
    "stop",
];

/// Elements whose text content is kept. Everywhere else, text is whitespace
/// between elements and is dropped.
const TEXTUAL: [&str; 4] = ["text", "tspan", "title", "desc"];

/// How an attribute's value is checked. The kind is the whole of the rule: an
/// attribute that can name a resource is held to a local reference, an
/// attribute that can carry CSS is held to a paint, and everything else is
/// geometry or an enumerated word.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Value {
    /// A name: letters, digits, `-` and `_`.
    Name,
    /// A colour: `none`, `currentColor`, a hex triple, `var(--token)`, or a
    /// local `url(#id)`.
    Paint,
    /// A local reference alone: `url(#id)` or `none`.
    Local,
    /// Numbers, separators, and the letters a path or a transform needs.
    Geometry,
    /// One word from the specification's own list.
    Word,
}

/// Every attribute that survives, and how its value is checked.
///
/// There is no `href`, no `xlink:href`, no `filter`, no `mask` and no `style`.
/// The first two fetch, the next two name a resource this list has no way to
/// vouch for, and `style` is CSS, which is a second language inside the first.
///
/// There is no `class` either: a class names the app's own styles.
const ATTRIBUTES: [(&str, Value); 47] = [
    ("id", Value::Name),
    ("viewbox", Value::Geometry),
    ("preserveaspectratio", Value::Geometry),
    ("width", Value::Geometry),
    ("height", Value::Geometry),
    ("x", Value::Geometry),
    ("y", Value::Geometry),
    ("x1", Value::Geometry),
    ("y1", Value::Geometry),
    ("x2", Value::Geometry),
    ("y2", Value::Geometry),
    ("cx", Value::Geometry),
    ("cy", Value::Geometry),
    ("r", Value::Geometry),
    ("rx", Value::Geometry),
    ("ry", Value::Geometry),
    ("dx", Value::Geometry),
    ("dy", Value::Geometry),
    ("d", Value::Geometry),
    ("points", Value::Geometry),
    ("transform", Value::Geometry),
    ("gradienttransform", Value::Geometry),
    ("offset", Value::Geometry),
    ("opacity", Value::Geometry),
    ("fill", Value::Paint),
    ("fill-opacity", Value::Geometry),
    ("fill-rule", Value::Word),
    ("stroke", Value::Paint),
    ("stroke-width", Value::Geometry),
    ("stroke-opacity", Value::Geometry),
    ("stroke-linecap", Value::Word),
    ("stroke-linejoin", Value::Word),
    ("stroke-dasharray", Value::Geometry),
    ("stroke-dashoffset", Value::Geometry),
    ("stroke-miterlimit", Value::Geometry),
    ("stop-color", Value::Paint),
    ("stop-opacity", Value::Geometry),
    ("clip-path", Value::Local),
    ("clippathunits", Value::Word),
    ("marker-start", Value::Local),
    ("marker-mid", Value::Local),
    ("marker-end", Value::Local),
    ("markerwidth", Value::Geometry),
    ("markerheight", Value::Geometry),
    ("refx", Value::Geometry),
    ("refy", Value::Geometry),
    ("orient", Value::Word),
];

/// Attributes that carry type, which only the text elements may take.
const TYPE_ATTRIBUTES: [(&str, Value); 8] = [
    ("font-size", Value::Geometry),
    ("font-style", Value::Word),
    ("font-weight", Value::Word),
    ("text-anchor", Value::Word),
    ("dominant-baseline", Value::Word),
    ("baseline-shift", Value::Word),
    ("letter-spacing", Value::Geometry),
    ("paint-order", Value::Word),
];

/// The colour an author writes for "the ink of the page", mapped to the token
/// that themes it. A figure follows light and dark for free.
const SENTINELS: [(&str, &str); 3] = [
    ("#000000", "currentColor"),
    ("#1a1a1a", "var(--graph-ink)"),
    ("#0000ff", "var(--graph-blue)"),
];

/// How deep a picture may nest, and how many elements it may hold. A file past
/// either is refused rather than drawn: both are far past any real figure.
const MAX_DEPTH: usize = 32;
const MAX_ELEMENTS: usize = 20_000;

/// Whether a tag opens an element or stands alone.
enum Kind {
    Start,
    Alone,
}

/// Re-emit an author's SVG as markup this app wrote.
///
/// # Errors
/// Returns what is wrong for the author to read: a file that is not an SVG at
/// all, one that will not parse, or one past the depth or element limits.
pub fn reemit(source: &str) -> Result<String, String> {
    // Every id this figure declares or names takes this prefix, so no id can
    // match one of the page's, or one of another figure's. The same file
    // gives the same prefix, and two copies of one figure name the same thing.
    let hash = blake3::hash(source.as_bytes()).to_hex();
    let scope = format!("s{}-", hash.as_str().get(..8).unwrap_or_default());
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = false;

    let mut out = String::with_capacity(source.len());
    // The elements whose start tag we wrote, so an end tag closes the right
    // one. A name we did not write never reaches this stack.
    let mut open: Vec<String> = Vec::new();
    // While skipping an element we do not know, how deep inside it we are.
    let mut skipping: usize = 0;
    let mut elements: usize = 0;
    let mut saw_root = false;

    loop {
        let event = reader
            .read_event()
            .map_err(|err| format!("this SVG will not parse: {err}"))?;
        let event_kind = match &event {
            Event::Start(_) => Kind::Start,
            _ => Kind::Alone,
        };
        match event {
            Event::Eof => break,
            Event::Start(tag) | Event::Empty(tag) => {
                // `<path … />` arrives as `Empty` and closes itself, so it
                // never joins the open stack and never nests a skip.
                let holds_children = matches!(event_kind, Kind::Start);
                let name = local_name(tag.name().as_ref());
                if skipping > 0 {
                    if holds_children {
                        skipping = skipping.saturating_add(1);
                    }
                    continue;
                }
                // The root is an `<svg>`, and the first element is the root.
                if !ELEMENTS.contains(&name.as_str()) || (!saw_root && name != "svg") {
                    // The element goes, and everything under it goes with it.
                    if holds_children {
                        skipping = 1;
                    }
                    continue;
                }
                saw_root = true;
                elements = elements.saturating_add(1);
                if elements > MAX_ELEMENTS {
                    return Err(format!(
                        "this SVG holds more than {MAX_ELEMENTS} elements: simplify it"
                    ));
                }
                if open.len() >= MAX_DEPTH {
                    return Err(format!(
                        "this SVG nests deeper than {MAX_DEPTH} levels: flatten it"
                    ));
                }
                let tag_name = written_name(&name);
                let root = open.is_empty();
                write_start(&mut out, &tag, &name, root, &scope);
                if holds_children {
                    out.push('>');
                    open.push(tag_name.to_owned());
                } else {
                    out.push_str("/>");
                    // A root that closes itself is the whole figure.
                    if root {
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
                    // The root is closed, and the figure with it. Anything
                    // after it would sit in the page, outside any `<svg>`.
                    if open.is_empty() {
                        break;
                    }
                }
            }
            Event::Text(text) => {
                if skipping > 0 || !in_text(&open) {
                    continue;
                }
                out.push_str(&escape_text(&text.xml10_content()));
            }
            // An entity arrives as its own event. The five XML names and a
            // numeric reference resolve to a character, which is then written
            // back escaped. Anything else is dropped: expanding an author's
            // own entity is how a parser is made to disagree with a browser.
            Event::GeneralRef(entity) => {
                if skipping > 0 || !in_text(&open) {
                    continue;
                }
                if let Some(character) = resolve_entity(&entity) {
                    out.push_str(&escape_text(&character.to_string()));
                }
            }
            // A comment, a processing instruction, a doctype and a CDATA
            // section all carry no geometry, so none of them is written.
            _ => {}
        }
    }

    // A start tag with no end tag (`<path … />`, which quick-xml reports as
    // `Empty`) leaves nothing open, so anything still open is ours to close.
    while let Some(name) = open.pop() {
        let _ = write!(out, "</{name}>");
    }
    if !saw_root {
        return Err("this file holds no <svg> element".to_owned());
    }
    Ok(out)
}

/// Whether the element being read holds text rather than geometry.
fn in_text(open: &[String]) -> bool {
    open.last()
        .is_some_and(|name| TEXTUAL.contains(&name.to_ascii_lowercase().as_str()))
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
        _ => None,
    }
}

/// The name this app writes for an element, in the case SVG expects.
///
/// XML is case-sensitive and `clipPath` is not `clippath`, so the two
/// camel-cased names are restored here. Every other allowed element is already
/// lowercase, and a name that reached this point is on the list.
fn written_name(lower: &str) -> &'static str {
    match lower {
        "clippath" => "clipPath",
        "lineargradient" => "linearGradient",
        other => ELEMENTS
            .iter()
            .find(|known| **known == other)
            .copied()
            .unwrap_or("g"),
    }
}

/// Write a start tag and its attributes, all but the `>`.
fn write_start(
    out: &mut String,
    tag: &quick_xml::events::BytesStart<'_>,
    name: &str,
    root: bool,
    scope: &str,
) {
    let _ = write!(out, "<{}", written_name(name));
    if root {
        out.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
        write_root_box(out, tag);
    }
    for attribute in tag.attributes().flatten() {
        if !root || !is_root_size(&attribute) {
            write_attribute(out, name, &attribute, scope);
        }
    }
}

/// Whether an attribute is the root's own `width` or `height`. The root takes
/// its size from the page and its shape from its `viewBox`, so a figure cannot
/// set itself wider than the card or taller than the screen.
fn is_root_size(attribute: &Attribute<'_>) -> bool {
    matches!(
        local_name(attribute.key.as_ref()).as_str(),
        "width" | "height"
    )
}

/// Give a root with no `viewBox` one from its `width` and `height`, where both
/// are plain numbers, so dropping them keeps the figure's shape.
fn write_root_box(out: &mut String, tag: &quick_xml::events::BytesStart<'_>) {
    let mut view_box = false;
    let mut width = None;
    let mut height = None;
    for attribute in tag.attributes().flatten() {
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Explicit1_0)
            .map(|value| value.trim().to_owned())
            .ok();
        match local_name(attribute.key.as_ref()).as_str() {
            "viewbox" => view_box = true,
            "width" => width = value,
            "height" => height = value,
            _ => {}
        }
    }
    let plain = |value: Option<String>| {
        value
            .map(|value| value.strip_suffix("px").unwrap_or(&value).to_owned())
            .filter(|value| {
                (1..=16).contains(&value.len())
                    && value.chars().all(|c| c.is_ascii_digit() || c == '.')
                    && value.matches('.').count() <= 1
            })
    };
    if !view_box && let (Some(width), Some(height)) = (plain(width), plain(height)) {
        let _ = write!(out, " viewBox=\"0 0 {width} {height}\"");
    }
}

/// Write one attribute, if this element may carry it and its value passes. An
/// `id`, and the id inside a `url(#…)`, take the figure's prefix.
fn write_attribute(out: &mut String, element: &str, attribute: &Attribute<'_>, scope: &str) {
    let name = local_name(attribute.key.as_ref());
    let textual = TEXTUAL.contains(&element);
    let Some(kind) = ATTRIBUTES
        .iter()
        .chain(textual.then_some(&TYPE_ATTRIBUTES).into_iter().flatten())
        .find(|(known, _)| *known == name)
        .map(|(_, kind)| *kind)
    else {
        return;
    };
    // The value arrives normalized, with the five predefined entities
    // resolved, so what is checked below is what a browser would see.
    let Ok(raw) = attribute.normalized_value(quick_xml::XmlVersion::Explicit1_0) else {
        return;
    };
    let value = raw.trim();
    if !passes(kind, value) {
        return;
    }
    let value = if name == "id" {
        format!("{scope}{value}")
    } else if is_local_url(value) {
        value.replacen("url(#", &format!("url(#{scope}"), 1)
    } else if kind == Value::Paint {
        theme(value)
    } else {
        value.to_owned()
    };
    let _ = write!(
        out,
        " {}=\"{}\"",
        written_attribute(&name),
        escape_attr(&value)
    );
}

/// The case an attribute is written in. Everything is lowercase in SVG but the
/// handful of camel-cased ones.
fn written_attribute(lower: &str) -> String {
    match lower {
        "viewbox" => "viewBox".to_owned(),
        "preserveaspectratio" => "preserveAspectRatio".to_owned(),
        "gradienttransform" => "gradientTransform".to_owned(),
        "clippathunits" => "clipPathUnits".to_owned(),
        "markerwidth" => "markerWidth".to_owned(),
        "markerheight" => "markerHeight".to_owned(),
        "refx" => "refX".to_owned(),
        "refy" => "refY".to_owned(),
        other => other.to_owned(),
    }
}

/// Whether a value is what its kind allows.
fn passes(kind: Value, value: &str) -> bool {
    if value.is_empty() || value.len() > 8192 {
        return false;
    }
    match kind {
        Value::Name => value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        Value::Word => {
            value.len() <= 32
                && value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '%')
        }
        Value::Geometry => value.chars().all(is_geometry),
        Value::Local => value == "none" || is_local_url(value),
        Value::Paint => {
            value == "none"
                || value.eq_ignore_ascii_case("currentcolor")
                || is_hex_colour(value)
                || is_token(value)
                || is_local_url(value)
        }
    }
}

/// The characters geometry is written with: numbers, separators, the path
/// commands, and the transform functions.
fn is_geometry(c: char) -> bool {
    c.is_ascii_alphanumeric() || " \t\r\n.,-+()%".contains(c)
}

/// `url(#id)`, and nothing else. A reference that leaves the document is what
/// turns a picture into a request.
fn is_local_url(value: &str) -> bool {
    value
        .strip_prefix("url(#")
        .and_then(|rest| rest.strip_suffix(')'))
        .is_some_and(|id| {
            !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa`.
fn is_hex_colour(value: &str) -> bool {
    value.strip_prefix('#').is_some_and(|hex| {
        matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
    })
}

/// `var(--token)`: the way a figure follows the theme.
fn is_token(value: &str) -> bool {
    value
        .strip_prefix("var(--")
        .and_then(|rest| rest.strip_suffix(')'))
        .is_some_and(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

/// Map the three colour sentinels onto the theme's tokens, so a figure drawn in
/// plain black follows the ink in both themes.
fn theme(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    SENTINELS
        .iter()
        .find(|(from, _)| *from == lower)
        .map_or_else(|| value.to_owned(), |(_, to)| (*to).to_owned())
}

/// An element or attribute name, without its namespace prefix, lowercased.
fn local_name(raw: &str) -> String {
    raw.rsplit(':').next().unwrap_or(raw).to_ascii_lowercase()
}

/// Escape text content this app writes.
fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escape a value this app writes into a double-quoted attribute.
fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::reemit;

    #[test]
    fn geometry_survives() {
        let out =
            reemit(r##"<svg viewBox="0 0 10 10"><path d="M0 0 L10 10" stroke="#000000"/></svg>"##)
                .expect("re-emits");
        assert!(out.contains("viewBox=\"0 0 10 10\""));
        assert!(out.contains("d=\"M0 0 L10 10\""));
        // The ink sentinel themes, so the figure follows light and dark.
        assert!(out.contains("stroke=\"currentColor\""), "{out}");
    }

    /// The whole point: a script is not filtered out of the author's markup,
    /// it is never written into ours, and its body goes with it.
    #[test]
    fn a_script_and_its_body_never_reach_the_page() {
        let out = reemit(
            "<svg><script>alert(1)</script><g><foreignObject><p>x</p></foreignObject></g></svg>",
        )
        .expect("re-emits");
        assert!(!out.contains("script"), "{out}");
        assert!(!out.contains("alert"), "{out}");
        assert!(!out.contains("foreignObject"), "{out}");
        assert!(!out.contains("<p>"), "{out}");
    }

    /// Anything that fetches or scripts through an attribute is refused by the
    /// value rule, not by a blocklist of schemes.
    #[test]
    fn a_link_out_of_the_document_is_refused() {
        let out = reemit(
            r#"<svg><a href="javascript:alert(1)"><circle cx="1" cy="1" r="1" fill="url(https://evil.example/x)" onclick="alert(1)"/></a></svg>"#,
        )
        .expect("re-emits");
        assert!(!out.contains("javascript"), "{out}");
        assert!(!out.contains("onclick"), "{out}");
        assert!(!out.contains("evil.example"), "{out}");
        // The circle sat inside an `<a>`, which is not on the list, so it goes
        // with its parent. Nothing is half-kept.
        assert!(!out.contains("circle"), "{out}");
    }

    /// A theme token is how a figure follows light and dark, so it passes.
    #[test]
    fn a_theme_token_passes() {
        let out =
            reemit(r#"<svg><line x1="0" y1="0" x2="1" y2="1" stroke="var(--graph-blue)"/></svg>"#)
                .expect("re-emits");
        assert!(out.contains("var(--graph-blue)"), "{out}");
    }

    /// Text is content, and it is escaped by us rather than forwarded.
    #[test]
    fn text_is_escaped_not_forwarded() {
        let out =
            reemit(r#"<svg><text x="0" y="0">a &lt; b &amp; c</text></svg>"#).expect("re-emits");
        assert!(out.contains("a &lt; b &amp; c"), "{out}");
    }

    /// Markup that is not a picture at all says so, rather than rendering a
    /// blank figure.
    #[test]
    fn a_file_that_is_not_an_svg_is_refused() {
        assert!(reemit("<html><body>hello</body></html>").is_err());
        assert!(reemit("not markup at all").is_err());
    }

    /// The audit's M1 probe: markup after the root closes would sit in the
    /// page outside any `<svg>`, so it is never written.
    #[test]
    fn nothing_after_the_root_is_written() {
        let out = reemit(
            r#"<svg viewBox="0 0 1 1"><rect width="1" height="1"/></svg><svg><g><text>after</text></g></svg><title>x</title>"#,
        )
        .expect("re-emits");
        assert_eq!(out.matches("<svg").count(), 1, "{out}");
        assert!(out.ends_with("</svg>"), "{out}");
        assert!(!out.contains("after") && !out.contains("<title"), "{out}");
    }

    /// An author's class could name the app's styles, so it goes. An id takes
    /// the figure's prefix, and so does every reference to it.
    #[test]
    fn ids_are_scoped_and_classes_dropped() {
        let out = reemit(
            r#"<svg viewBox="0 0 4 4"><defs><marker id="arrow"><path d="M0 0"/></marker></defs><line class="sheet" x1="0" y1="0" x2="4" y2="4" marker-end="url(#arrow)" fill="url(#arrow)"/></svg>"#,
        )
        .expect("re-emits");
        assert!(!out.contains("class"), "{out}");
        assert!(!out.contains("id=\"arrow\""), "{out}");
        let declared = out
            .split("id=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("the marker keeps an id");
        assert!(
            declared.starts_with('s') && declared.ends_with("-arrow"),
            "{out}"
        );
        assert!(
            out.contains(&format!("marker-end=\"url(#{declared})\"")),
            "{out}"
        );
        assert!(out.contains(&format!("fill=\"url(#{declared})\"")), "{out}");
    }

    /// The root takes its size from the page: its `width` and `height` go, and
    /// a root with no `viewBox` takes one from them.
    #[test]
    fn the_root_size_gives_way_to_its_view_box() {
        let out = reemit(r#"<svg width="100000" height="100000" viewBox="0 0 4 4"><rect width="4" height="4"/></svg>"#)
            .expect("re-emits");
        assert!(!out.contains("100000"), "{out}");
        assert!(out.contains("<rect width=\"4\" height=\"4\"/>"), "{out}");
        let out = reemit(r#"<svg width="40px" height="30"><rect width="4" height="4"/></svg>"#)
            .expect("re-emits");
        assert!(
            out.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 40 30\">"),
            "{out}"
        );
    }

    /// An entity that expands to a payload cannot smuggle markup: the value is
    /// unescaped, checked, and written back escaped.
    #[test]
    fn an_entity_cannot_break_out_of_an_attribute() {
        let out =
            reemit(r#"<svg><rect id="a&quot;onload=&quot;alert(1)" width="1" height="1"/></svg>"#)
                .expect("re-emits");
        assert!(!out.contains("onload"), "{out}");
    }
}
