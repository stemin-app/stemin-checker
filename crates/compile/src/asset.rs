//! Inlining an asset: a picture becomes part of the card that shows it.
//!
//! Nothing fetches a side asset at runtime, so a raster picture
//! is inlined as a `data:` URI and an author's SVG is inlined as markup this
//! app re-emitted ([`crate::svg`]). Offline then works with no blob URLs, no
//! asset store and no service worker involved, and the diff already covers a
//! changed image, because an asset is a file with a content hash like any
//! other.
//!
//! An SVG is markup rather than a `data:` URI for one reason: a picture inside
//! `<img>` cannot take `currentColor`, and a figure must follow the theme,
//! light or dark.

use std::fmt::Write as _;

use crate::b64::encode;
use crate::render::escape_attr;

/// The media type a file extension names, for the `data:` URI.
fn media_type(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return None,
    })
}

/// Inline one asset into the card that references it.
///
/// # Errors
/// Returns what is wrong for the author to read: a type the format does not
/// carry, or an SVG the allowlist refuses.
pub fn inline(path: &str, bytes: &[u8], alt: &str) -> Result<String, String> {
    let ext = path
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    if ext == "svg" {
        let source = std::str::from_utf8(bytes)
            .map_err(|_| format!("`{path}` is not UTF-8 text, and an SVG must be"))?;
        let markup = crate::svg::reemit(source).map_err(|why| format!("`{path}`: {why}"))?;
        let opened = format!(
            "<svg class=\"asset\" role=\"img\" aria-label=\"{}\"",
            escape_attr(alt)
        );
        return Ok(markup.replacen("<svg", &opened, 1));
    }
    let Some(media) = media_type(&ext) else {
        return Err(format!(
            "`{path}` is not an asset this format carries: use .png, .jpg, .jpeg, .webp or .svg"
        ));
    };
    Ok(format!(
        "<img class=\"asset\" alt=\"{}\" src=\"data:{media};base64,{}\"/>",
        escape_attr(alt),
        encode(bytes),
    ))
}

/// A `data:` URI for one file, for a caller that wants the URI alone.
#[must_use]
pub fn data_uri(media: &str, bytes: &[u8]) -> String {
    let mut out = String::new();
    let _ = write!(out, "data:{media};base64,{}", encode(bytes));
    out
}

#[cfg(test)]
mod tests {
    use super::inline;

    #[test]
    fn a_raster_asset_becomes_a_data_uri() {
        let html = inline("assets/x.png", b"\x89PNG", "A trace").expect("inlines");
        assert!(html.contains("src=\"data:image/png;base64,"), "{html}");
        assert!(html.contains("alt=\"A trace\""), "{html}");
    }

    /// An SVG is markup, so it can take the ink of the page. It is the markup
    /// this app wrote, never the author's bytes.
    #[test]
    fn an_svg_asset_becomes_themed_markup() {
        let svg = r##"<svg viewBox="0 0 4 4"><script>alert(1)</script><line x1="0" y1="0" x2="4" y2="4" stroke="#000000"/></svg>"##;
        let html = inline("assets/x.svg", svg.as_bytes(), "A line").expect("inlines");
        assert!(html.starts_with("<svg class=\"asset\""), "{html}");
        assert!(html.contains("aria-label=\"A line\""), "{html}");
        assert!(html.contains("stroke=\"currentColor\""), "{html}");
        assert!(!html.contains("script"), "{html}");
    }

    #[test]
    fn an_unknown_type_is_refused() {
        assert!(inline("assets/x.gif", b"GIF89a", "").is_err());
    }
}
