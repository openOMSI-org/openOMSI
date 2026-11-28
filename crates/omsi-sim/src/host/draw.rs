//! The callbacks that draw: the fonts (`GetFontIndex`, `TextLength`) and the `ST*` calls on the
//! `[scripttexture]` images, with the pictures `STLoadTex` loads.

use super::*;

/// `STLoadTex`: retain the most recently used destination bitmaps rather than
/// clearing every cached image when a bus changes its display.
type STImage = Result<Arc<crate::image_decode::DecodedImage>, String>;
const ST_CACHE_BYTES: usize = 64 << 20;
const ST_CACHE_ENTRIES: usize = 512;

#[derive(Default)]
struct STCache {
    map: std::collections::HashMap<std::path::PathBuf, (STImage, u64)>,
    bytes: usize,
    clock: u64,
}

impl STCache {
    fn get(&mut self, path: &std::path::Path) -> Option<STImage> {
        self.clock = self.clock.wrapping_add(1);
        let (image, used) = self.map.get_mut(path)?;
        *used = self.clock;
        Some(image.clone())
    }

    fn insert(&mut self, path: std::path::PathBuf, image: STImage) {
        let bytes = image.as_ref().map_or(0, |i| i.rgba.len());
        // A single oversized image may be drawn but must not displace the cache.
        if bytes > ST_CACHE_BYTES {
            return;
        }
        self.clock = self.clock.wrapping_add(1);
        if let Some((previous, _)) = self.map.insert(path, (image, self.clock)) {
            self.bytes = self
                .bytes
                .saturating_sub(previous.as_ref().map_or(0, |i| i.rgba.len()));
        }
        self.bytes = self.bytes.saturating_add(bytes);
        while self.bytes > ST_CACHE_BYTES || self.map.len() > ST_CACHE_ENTRIES {
            let oldest = self
                .map
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(path, _)| path.clone());
            let Some(oldest) = oldest else { break };
            if let Some((previous, _)) = self.map.remove(&oldest) {
                self.bytes = self
                    .bytes
                    .saturating_sub(previous.as_ref().map_or(0, |i| i.rgba.len()));
            }
        }
    }
}

fn st_load(path: &std::path::Path) -> STImage {
    static CACHE: std::sync::OnceLock<Mutex<STCache>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().get(path) {
        return hit;
    }
    let got = crate::image_decode::decode(path).map(Arc::new);
    cache.lock().insert(path.to_path_buf(), got.clone());
    got
}

impl VehicleHost {
    /// A drawing callback (see [`VehicleHost::callback`], which hands only those here).
    pub(super) fn draw_callback(&mut self, cb: Option<Cb>, stacks: &mut Stacks) {
        let draw_script_textures = !self.remote_script_textures;
        match cb {
            Some(Cb::GetFontIndex) => {
                let font = stacks.pop_str();
                let idx = match self
                    .fonts
                    .entries
                    .iter()
                    .position(|f| f.0.eq_ignore_ascii_case(&font))
                {
                    Some(i) => i,
                    None => {
                        // an empty name or a depot string that is no font (the Krüger
                        // matrix asks for the custom fonts of termini 14991.. this way) is
                        // looked up once and remembered as missing
                        let atlas = if font.trim().is_empty() {
                            None
                        } else {
                            self.font_lib.as_ref().and_then(|l| l.lock().load(&font))
                        };
                        self.fonts.entries.push((font, atlas));
                        self.fonts.entries.len() - 1
                    }
                };
                // OMSI answers -1 for a font it does not have, and scripts rely on it: the
                // Krüger matrix falls back from a missing custom font to the one before with
                // `1 + 31416 * l1 1 + max 31415 % 1 -`, which only works for -1. An index for
                // a font that cannot draw left the destination display blank.
                let found = self.fonts.entries[idx].1.is_some();
                stacks.push(if found { idx as f32 } else { -1.0 });
            }
            Some(Cb::TextLength) => {
                let font = arg_i32(stacks.pop());
                let text = stacks.pop_str();
                // as Omsi.exe measures it (0x5d6c00): the glyphs and the gaps between them
                let w = self
                    .font_atlas(font)
                    .map(|a| a.font.text_width(&text) as f32)
                    .unwrap_or(0.0);
                if debug_text() {
                    log::info!(
                        "TextLength(font {font} = {:?}, {text:?}) = {w}",
                        self.font_atlas(font).map(|a| a.font.name.clone())
                    );
                }
                stacks.push(w);
            }
            // --- script textures (matrix displays)
            Some(Cb::StNewTex) => {
                let i = arg_idx(stacks.pop());
                if let Some(t) = self
                    .script_textures
                    .get_mut(i)
                    .filter(|_| draw_script_textures)
                {
                    t.renew();
                }
            }
            Some(Cb::StLock) => {
                let i = arg_idx(stacks.pop());
                if let Some(t) = self
                    .script_textures
                    .get_mut(i)
                    .filter(|_| draw_script_textures)
                {
                    t.locked = true;
                }
            }
            Some(Cb::StUnlock) => {
                let i = arg_idx(stacks.pop());
                if let Some(t) = self
                    .script_textures
                    .get_mut(i)
                    .filter(|_| draw_script_textures)
                {
                    t.unlock();
                }
            }
            Some(Cb::StFilter) => {
                // OMSI creates the mip chain only when the script asks it to, normally
                // immediately after STUnlock. Remember that request for the renderer; it
                // must also refresh the chain when this matrix changes again.
                let i = arg_idx(stacks.pop());
                if let Some(t) = self
                    .script_textures
                    .get_mut(i)
                    .filter(|_| draw_script_textures)
                {
                    if !t.mipmaps {
                        t.mipmaps = true;
                        t.dirty = true;
                    }
                }
            }
            Some(Cb::StSetColor) => {
                let b = stacks.pop();
                let g = stacks.pop();
                let r = stacks.pop();
                let a = stacks.pop();
                let i = arg_idx(stacks.pop());
                if let Some(t) = self
                    .script_textures
                    .get_mut(i)
                    .filter(|_| draw_script_textures)
                {
                    t.color = [r, g, b, a].map(|c| arg_i32(c).clamp(0, 255) as u8);
                }
            }
            Some(Cb::StDrawPixel) => {
                let y = arg_i32(stacks.pop());
                let x = arg_i32(stacks.pop());
                let i = arg_idx(stacks.pop());
                if let Some(t) = self
                    .script_textures
                    .get_mut(i)
                    .filter(|_| draw_script_textures)
                {
                    let c = t.color;
                    t.put(x, y, c);
                }
            }
            Some(Cb::StDrawRect) => {
                let y2 = arg_i32(stacks.pop());
                let x2 = arg_i32(stacks.pop());
                let y1 = arg_i32(stacks.pop());
                let x1 = arg_i32(stacks.pop());
                let i = arg_idx(stacks.pop());
                if let Some(t) = self
                    .script_textures
                    .get_mut(i)
                    .filter(|_| draw_script_textures)
                {
                    t.rect(x1, y1, x2, y2);
                }
            }
            Some(Cb::StTextOut) => {
                let text = stacks.pop_str();
                let spacing = arg_i32(stacks.pop());
                let mode = stacks.pop();
                let font = arg_i32(stacks.pop());
                let y = arg_i32(stacks.pop());
                let x = arg_i32(stacks.pop());
                let i = arg_idx(stacks.pop());
                if !draw_script_textures {
                    return;
                }
                let atlas = self.font_atlas(font);
                if debug_text() {
                    log::info!(
                        "STTextOut(tex {i}, x {x}, y {y}, font {font}, spacing {spacing}, {text:?}) atlas {:?} from {:?} bitmap {:?} {}x{} E {:?}",
                        atlas.as_ref().map(|a| a.font.name.clone()),
                        atlas.as_ref().map(|a| a.font.path.clone()),
                        atlas.as_ref().map(|a| a.font.alpha.clone()),
                        atlas.as_ref().map(|a| a.width).unwrap_or(0),
                        atlas.as_ref().map(|a| a.height).unwrap_or(0),
                        atlas
                            .as_ref()
                            .and_then(|a| a.font.glyph('E').map(|g| (g.x0, g.x1, g.y)))
                    );
                }
                if let (Some(t), Some(a)) = (
                    self.script_textures
                        .get_mut(i)
                        .filter(|_| draw_script_textures),
                    atlas,
                ) {
                    t.text_out(&a, x, y, spacing, arg_i32(mode) as u8, &text);
                }
            }
            Some(Cb::StReadPixel) => {
                let y = arg_i32(stacks.pop());
                let x = arg_i32(stacks.pop());
                let i = arg_idx(stacks.pop());
                // `STReadPixel` makes the selected texture's current ST colour the
                // pixel it read.  RHLib then asks `STGet*` of either that source texture
                // or a different target texture: the latter must retain its own draw
                // colour while the source keeps changing beneath the scaler.
                if let Some(t) = self.script_textures.get_mut(i) {
                    let color = t.get(x, y);
                    t.color = color;
                }
            }
            Some(Cb::StGetR) | Some(Cb::StGetG) | Some(Cb::StGetB) | Some(Cb::StGetA) => {
                let i = arg_idx(stacks.pop());
                let k = match cb {
                    Some(Cb::StGetR) => 0,
                    Some(Cb::StGetG) => 1,
                    Some(Cb::StGetB) => 2,
                    _ => 3,
                };
                stacks.push(
                    self.script_textures
                        .get(i)
                        .map(|t| t.color[k] as f32)
                        .unwrap_or(0.0),
                );
            }
            Some(Cb::StCopyColor) => {
                // colour of texture a → texture b
                let b = arg_idx(stacks.pop());
                let a = arg_idx(stacks.pop());
                if let Some(c) = self.script_textures.get(a).map(|t| t.color) {
                    if let Some(t) = self
                        .script_textures
                        .get_mut(b)
                        .filter(|_| draw_script_textures)
                    {
                        t.color = c;
                    }
                }
            }
            Some(Cb::StLoadTex) => {
                let i = arg_idx(stacks.pop());
                let path = stacks.pop_str();
                if !draw_script_textures {
                    return;
                }
                // relative to the vehicle's texture folder, like every texture name of the
                // vehicle: the Krüger matrix loads `..\..\Anzeigen\Krueger\<bitmap>`,
                // which is `Vehicles\Anzeigen\...` seen from `Vehicles\<bus>\Texture`
                // (and found in whichever content root has it)
                let mut full = omsi_cfg::resolve_path(
                    &omsi_cfg::resolve_path(&self.content_dir, "Texture"),
                    &path,
                );
                // (then the global `Texture` folder, as OMSI looks)
                if !omsi_cfg::vfs::is_file(&full) {
                    if let Some((_, p)) =
                        omsi_cfg::find_in_roots(&format!("Texture/{}", path.replace('\\', "/")))
                    {
                        full = p;
                    }
                }
                // (OMSI pushes nothing: a file it cannot find goes to its log only)
                match st_load(&full) {
                    Ok(img) => {
                        if let Some(t) = self
                            .script_textures
                            .get_mut(i)
                            .filter(|_| draw_script_textures)
                        {
                            t.load(img.width, img.height, &img.rgba);
                        }
                    }
                    Err(e) => log::warn!(
                        "STLoadTex: did not find texture file {}: {e}",
                        full.display()
                    ),
                }
            }
            _ => unreachable!("not a drawing callback: {cb:?}"),
        }
    }
}

#[cfg(test)]
mod script_texture_cache_tests {
    use super::*;

    #[test]
    fn missing_script_pictures_have_bounded_lru_cache() {
        let mut cache = STCache::default();
        for n in 0..ST_CACHE_ENTRIES {
            cache.insert(
                std::path::PathBuf::from(format!("missing-{n}")),
                Err("missing".into()),
            );
        }
        let first = std::path::Path::new("missing-0");
        assert!(cache.get(first).is_some());
        cache.insert("new-entry".into(), Err("missing".into()));
        assert!(cache.get(first).is_some());
        assert!(cache.get(std::path::Path::new("missing-1")).is_none());
        assert_eq!(cache.map.len(), ST_CACHE_ENTRIES);
        assert_eq!(cache.bytes, 0);
    }
}
