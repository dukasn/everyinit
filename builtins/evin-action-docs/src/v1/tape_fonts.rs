// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Immutable IBM Plex Mono assets used exclusively by terminal tapes.

use base64::Engine;
use std::io;
use std::path::Path;

pub struct TapeFont {
    pub filename: &'static str,
    pub weight: u16,
    pub style: &'static str,
    pub bytes: &'static [u8],
    pub sha256: &'static str,
}

pub const FONTS: [TapeFont; 4] = [
    TapeFont {
        filename: "IBMPlexMono-Regular.woff2",
        weight: 400,
        style: "normal",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/IBMPlexMono-Regular.woff2"
        )),
        sha256: "49ce58b41a0e1cb921c0f58d9a5b8b96a2cc21437c7066f3ba4f24873076d131",
    },
    TapeFont {
        filename: "IBMPlexMono-Italic.woff2",
        weight: 400,
        style: "italic",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/IBMPlexMono-Italic.woff2"
        )),
        sha256: "6afc2a6edd9a1d1f8104daf139a5062392f47da2f97fe19cb18a6a5a1fa67ec3",
    },
    TapeFont {
        filename: "IBMPlexMono-Bold.woff2",
        weight: 700,
        style: "normal",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/IBMPlexMono-Bold.woff2"
        )),
        sha256: "5788454f0ba4bd6300752c474215c4dd926682fa173ae1c6252d57828b6a235d",
    },
    TapeFont {
        filename: "IBMPlexMono-BoldItalic.woff2",
        weight: 700,
        style: "italic",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/IBMPlexMono-BoldItalic.woff2"
        )),
        sha256: "b0bfaa05c90c662c164ca0515ec42baddac93129eab1af067f8872d84f10df46",
    },
];

pub fn css(single_file: bool) -> String {
    let mut css = String::new();
    for font in &FONTS {
        let source = if single_file {
            format!(
                "data:font/woff2;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(font.bytes)
            )
        } else {
            format!("fonts/{}", font.filename)
        };
        css.push_str(&format!(
            "@font-face{{font-family:'IBM Plex Mono';src:url('{source}') format('woff2');font-display:block;font-style:{};font-weight:{};font-synthesis:none}}",
            font.style, font.weight
        ));
    }
    css
}

pub fn publish(out_dir: &Path) -> io::Result<usize> {
    let fonts_dir = out_dir.join("assets/fonts");
    std::fs::create_dir_all(&fonts_dir)?;
    for font in &FONTS {
        std::fs::write(fonts_dir.join(font.filename), font.bytes)?;
    }
    Ok(FONTS.len())
}

/// The Matter shell inlines its stylesheet, so relative font URLs must be
/// resolved from each rendered page rather than from `assets/theme.css`.
pub fn with_page_asset_prefix(css: &str, assets_prefix: &str) -> String {
    let mut page_css = css.to_string();
    for font in &FONTS {
        let local_url = format!("url('fonts/{}')", font.filename);
        let page_url = format!("url('{}fonts/{}')", assets_prefix, font.filename);
        page_css = page_css.replace(&local_url, &page_url);
    }
    page_css
}

/// Return the faces actually used by a page's generated tape SVG markup.
/// Preloading unused italic/bold files makes browsers warn and wastes the
/// connection; the regular face is always needed for a tape.
pub fn used_by_markup(markup: &str) -> Vec<&'static TapeFont> {
    let mut bold = false;
    let mut italic = false;
    let mut bold_italic = false;
    let mut rest = markup;
    while let Some(start) = rest.find("<tspan") {
        let Some(end) = rest[start..].find('>') else {
            break;
        };
        let attributes = &rest[start..start + end];
        let is_bold =
            attributes.contains("font-weight=\"700\"") || attributes.contains("term-bold");
        let is_italic =
            attributes.contains("font-style=\"italic\"") || attributes.contains("term-italic");
        match (is_bold, is_italic) {
            (true, true) => bold_italic = true,
            (true, false) => bold = true,
            (false, true) => italic = true,
            (false, false) => {}
        }
        rest = &rest[start + end + 1..];
    }
    FONTS
        .iter()
        .filter(|font| match (font.weight, font.style) {
            (400, "normal") => true,
            (700, "normal") => bold,
            (400, "italic") => italic,
            (700, "italic") => bold_italic,
            _ => false,
        })
        .collect()
}

/// Remove only the four generated tape faces when a rebuilt site no longer
/// needs separated font assets. This keeps a previous tape build from making
/// a no-tape or single-file build appear to publish the fonts.
pub fn remove_published(out_dir: &Path) -> io::Result<()> {
    let fonts_dir = out_dir.join("assets/fonts");
    for font in &FONTS {
        match std::fs::remove_file(fonts_dir.join(font.filename)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    match std::fs::remove_dir(&fonts_dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn bundled_fonts_are_unmodified_woff2_files() {
        for font in &FONTS {
            assert_eq!(&font.bytes[..4], b"wOF2", "{}", font.filename);
            assert_eq!(format!("{:x}", Sha256::digest(font.bytes)), font.sha256);
        }
    }

    #[test]
    fn css_uses_external_or_embedded_fonts_as_requested() {
        let external = css(false);
        assert!(external.contains("fonts/IBMPlexMono-Regular.woff2"));
        assert!(!external.contains("data:font/woff2"));
        let inline = css(true);
        assert!(inline.contains("data:font/woff2;base64,"));
        assert!(inline.contains("font-display:block"));
    }

    #[test]
    fn page_css_resolves_separated_fonts_from_the_page() {
        let css = with_page_asset_prefix(&css(false), "../assets/");
        assert!(css.contains("url('../assets/fonts/IBMPlexMono-Regular.woff2')"));
        assert!(!css.contains("url('fonts/IBMPlexMono-Regular.woff2')"));
    }

    #[test]
    fn preloads_only_the_faces_used_by_tape_markup() {
        let regular = used_by_markup("<svg></svg>");
        assert_eq!(regular.len(), 1);
        assert_eq!(regular[0].filename, "IBMPlexMono-Regular.woff2");

        let styled = used_by_markup(
            "<tspan font-weight=\"700\"></tspan><tspan font-style=\"italic\"></tspan><tspan font-weight=\"700\" font-style=\"italic\"></tspan>",
        );
        assert_eq!(styled.len(), 4);
    }
}
