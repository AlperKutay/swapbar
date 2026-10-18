//! Embedded assets (Hugeicons stroke-rounded, MIT — see assets/icons/LICENSE-hugeicons.md).

use gpui::{AssetSource, Hsla, IntoElement, Result, SharedString, Styled, px, svg};
use std::borrow::Cow;

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        fn lookup(path: &str) -> Option<&'static [u8]> {
            match path {
                $(concat!("icons/", $name, ".svg") => Some(include_bytes!(concat!("../assets/icons/", $name, ".svg"))),)*
                _ => None,
            }
        }
        const ALL: &[&str] = &[$(concat!("icons/", $name, ".svg")),*];
    };
}

icons!(
    "settings",
    "settings-small",
    "laptop",
    "users",
    "swap",
    "info",
    "refresh",
    "power",
    "external",
    "github",
    "globe",
    "x",
    "instagram",
    "coins",
    "rocket",
    "logomark"
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(lookup(path).map(Cow::Borrowed))
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ALL
            .iter()
            .filter(|p| p.starts_with(path))
            .map(|p| SharedString::from(*p))
            .collect())
    }
}

/// A Hugeicons glyph tinted with `color`.
pub fn icon(name: &str, size: f32, color: Hsla) -> impl IntoElement {
    svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(px(size))
        .text_color(color)
        .flex_shrink_0()
}
