//! The `[htmltexture]` pages a model declares: the lookup every page's consumer asks.

use super::*;

impl Model {
    /// The `[htmltexture]` page a script texture index stands for, when it is one: a slot
    /// is bound to a page with `[useHtmlTexture]`, or with `[useScriptTexture]` naming
    /// the page's index. Every consumer of a page asks here.
    pub fn html_page(&self, script_index: usize) -> Option<&HtmlTextureDef> {
        self.html_textures
            .iter()
            .find(|h| h.script_index == script_index)
    }

    /// Whether that page declares itself a surface (`lit` after its path): the word is
    /// the page's own, so every place the page reaches a material follows it - a scenery
    /// object's slots and a vehicle's alike (see `docs/HTMLTEXTURE.md`).
    pub fn html_page_lit(&self, script_index: usize) -> bool {
        self.html_page(script_index).is_some_and(|h| h.lit)
    }
}
