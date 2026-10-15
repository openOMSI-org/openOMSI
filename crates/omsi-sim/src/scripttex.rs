//! `[scripttexture]`: RGBA images the scripts draw into through the `ST*` callbacks
//! (matrix displays). Materials use them with `[useScriptTexture] n` or as a transparency
//! map `[matl_transmap] \S:n`.

use omsi_content::font::FontAtlas;
use std::sync::Arc;

pub struct ScriptTexture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Drawing buffer changed since the last unlock or upload.
    pub dirty: bool,
    pub locked: bool,
    /// Last image released by STUnlock, retained even when the script locks again
    /// before the renderer runs (the Atron AFR4 keeps its drawing buffer locked).
    pending: Option<Vec<u8>>,
    /// `STFilter` was called after this texture was unlocked. OMSI then generates a mip
    /// chain, which keeps a distant matrix or IBIS screen stable instead of sampling its
    /// full-resolution pixels directly.
    pub mipmaps: bool,
    /// Current draw colour as set by `STSetColor` (r, g, b, a).
    pub color: [u8; 4],
    /// The picture comes from another player's game (`feed`): what the scripts draw here
    /// is not shown - the copy's own scrolling fought the sender's picture.
    pub fed: bool,
    /// The size the `[scripttexture]` entry declares, which `STNewTex` comes back to
    /// (`STLoadTex` makes the texture the size of its file).
    declared: (u32, u32),
}

impl ScriptTexture {
    pub fn new(width: i32, height: i32) -> ScriptTexture {
        let (w, h) = (width.max(1) as u32, height.max(1) as u32);
        ScriptTexture {
            width: w,
            height: h,
            rgba: vec![0; (w * h * 4) as usize],
            dirty: true,
            locked: false,
            pending: None,
            mipmaps: false,
            color: [255, 255, 255, 255],
            fed: false,
            declared: (w, h),
        }
    }

    /// Show the picture another player's scripts drew on their bus (`width` x `height`,
    /// RGBA), from now on instead of what the scripts here draw. Nothing changes for a
    /// picture equal to the one shown.
    pub fn feed(&mut self, width: u32, height: u32, rgba: Vec<u8>) {
        if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
            return;
        }
        let same = self.fed && (width, height) == (self.width, self.height) && rgba == self.rgba;
        self.fed = true;
        self.locked = false;
        self.dirty = false;
        if same {
            return;
        }
        self.width = width;
        self.height = height;
        self.rgba = rgba;
        self.rgba.truncate((width * height * 4) as usize);
        self.pending = Some(self.rgba.clone());
    }

    /// `STNewTex`: a new, empty texture of the declared size (TComplMapObjInst.v06 case 0
    /// releases it and calls D3DXCreateTexture with the `[scripttexture]` entry's size).
    pub fn renew(&mut self) {
        let (w, h) = self.declared;
        if (w, h) != (self.width, self.height) {
            self.width = w;
            self.height = h;
            self.rgba = vec![0; (w * h * 4) as usize];
            // (an image released at the old size must not go up at the new one)
            self.pending = None;
        }
        self.clear();
        // `STNewTex` starts a new drawing surface.  RHLib's transparency-map scaler
        // checks the target's current alpha before its first `STSetColor`; this must
        // therefore match the new transparent canvas rather than the prior draw state.
        self.color = [0; 4];
    }

    /// Put back a picture saved with a situation (`width` x `height`, RGBA), as if the
    /// script had drawn it and let it go: a script that redraws only when its texts change
    /// draws nothing after a load, and the display stayed empty (#1559). A picture of
    /// another size than the texture's is not taken.
    pub fn restore(&mut self, width: u32, height: u32, rgba: Vec<u8>) -> bool {
        if (width, height) != (self.width, self.height) || rgba.len() != (width * height * 4) as usize {
            return false;
        }
        self.rgba = rgba;
        self.locked = false;
        self.pending = Some(self.rgba.clone());
        self.dirty = false;
        true
    }

    /// A picture an `.opl` plugin drew (`AccessScriptTexture`), in place of the texture's:
    /// taken only at the texture's own size. It goes up as if a script had let it go - while
    /// a script holds the texture locked, as the image released, so the script's half-done
    /// drawing does not show and its next unlock draws over the plugin's again.
    pub fn replace(&mut self, width: u32, height: u32, rgba: &[u8]) -> bool {
        if (width, height) != (self.width, self.height) || rgba.len() != self.rgba.len() {
            return false;
        }
        if self.locked {
            match &mut self.pending {
                Some(p) if p.len() == rgba.len() => p.copy_from_slice(rgba),
                p => *p = Some(rgba.to_vec()),
            }
        } else {
            self.rgba.copy_from_slice(rgba);
            self.pending = None;
            self.dirty = true;
        }
        true
    }

    pub fn clear(&mut self) {
        self.rgba.iter_mut().for_each(|b| *b = 0);
        self.dirty = true;
    }

    pub fn unlock(&mut self) {
        self.locked = false;
        if self.fed {
            self.dirty = false;
            return;
        }
        if self.dirty {
            // (into the buffer of an image released before and not yet taken, when there
            // is one: a script that unlocks several times a frame made a new copy each time)
            match &mut self.pending {
                Some(p) => p.clone_from(&self.rgba),
                None => self.pending = Some(self.rgba.clone()),
            }
            self.dirty = false;
        }
    }

    /// Take the latest released image. A locked buffer may already contain edits
    /// for the next unlock; those must not replace the image just released.
    pub fn take_upload(&mut self) -> Option<Vec<u8>> {
        if self.fed {
            self.dirty = false;
            return self.pending.take();
        }
        if !self.locked && self.dirty {
            self.dirty = false;
            self.pending = None;
            Some(self.rgba.clone())
        } else {
            self.pending.take()
        }
    }

    pub fn put(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        self.rgba[i..i + 4].copy_from_slice(&c);
        self.dirty = true;
    }

    pub fn get(&self, x: i32, y: i32) -> [u8; 4] {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return [0; 4];
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }

    pub fn rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let c = self.color;
        for y in y1.min(y2)..=y1.max(y2) {
            for x in x1.min(x2)..=x1.max(x2) {
                self.put(x, y, c);
            }
        }
    }

    /// Draw `text` with the font at (x, y) (top-left), `spacing` extra pixels between
    /// letters. Glyph coverage scales the colour's alpha; RGB is the draw colour.
    /// `STTextOut` as OMSI draws (0x7bb731, the original): `mode` bit 0 takes the colour
    /// from the font's colour bitmap (else the `STSetColor` colour), the alpha is the font's
    /// mask; `mode & 3 == 2` writes only the glyphs' covered pixels (a transparent
    /// background), any other mode the whole glyph cell.
    pub fn text_out(&mut self, atlas: &FontAtlas, x: i32, y: i32, spacing: i32, mode: u8, text: &str) {
        let glyph_h = atlas.font.height.max(1) as i32;
        let mut cx = x;
        let c = self.color;
        let from_font = mode & 1 != 0;
        let transparent = mode & 3 == 2;
        for ch in text.chars() {
            if ch.is_whitespace() {
                cx += atlas.font.space_width() + atlas.font.gap + spacing;
                continue;
            }
            let Some(g) = atlas.font.glyph(ch) else {
                cx += atlas.font.gap.max(1) + spacing;
                continue;
            };
            let gw = (g.x1 - g.x0).max(0);
            for gy in 0..glyph_h {
                let sy = g.y + gy;
                if sy < 0 || sy >= atlas.height as i32 {
                    continue;
                }
                for gx in 0..gw {
                    let sx = g.x0 + gx;
                    if sx < 0 || sx >= atlas.width as i32 {
                        continue;
                    }
                    let si = ((sy as u32 * atlas.width + sx as u32) * 4) as usize;
                    let cov = atlas.alpha[si];
                    if cov == 0 && transparent {
                        continue;
                    }
                    let rgb = if from_font && si + 2 < atlas.color.len() { [atlas.color[si], atlas.color[si + 1], atlas.color[si + 2]] } else { [c[0], c[1], c[2]] };
                    self.put(cx + gx, y + gy, [rgb[0], rgb[1], rgb[2], cov]);
                }
            }
            cx += gw + atlas.font.gap + spacing;
        }
    }

    /// `STLoadTex`: the texture becomes the file's picture. Omsi.exe (TComplMapObjInst.v06,
    /// 0x7bbb08) releases it and calls D3DXCreateTextureFromFileExA with width and height
    /// D3DX_DEFAULT and a full mip chain: the file's own size rounded up to a power of two,
    /// the picture stretched to fill it, so the mesh's UVs always cover the whole bitmap
    /// (a 256x64 bitmap into a 256x32 entry is not cut to its top half).
    pub fn load(&mut self, w: u32, h: u32, rgba: &[u8]) {
        if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
            return;
        }
        let (tw, th) = (w.next_power_of_two(), h.next_power_of_two());
        self.width = tw;
        self.height = th;
        self.rgba = if (tw, th) == (w, h) { rgba[..(w * h * 4) as usize].to_vec() } else { resample(w, h, rgba, tw, th) };
        self.mipmaps = true;
        // the file replaces whatever was released at the old size
        self.pending = None;
        self.dirty = true;
    }
}

/// Stretch an RGBA picture to another size, bilinearly (texel centres onto texel centres).
fn resample(w: u32, h: u32, src: &[u8], tw: u32, th: u32) -> Vec<u8> {
    let mut out = vec![0u8; (tw * th * 4) as usize];
    let axis = |t: u32, n: u32, size: u32| -> (usize, usize, f32) {
        let f = ((t as f32 + 0.5) * n as f32 / size as f32 - 0.5).clamp(0.0, (n - 1) as f32);
        let i = f.floor() as usize;
        (i, (i + 1).min(n as usize - 1), f - i as f32)
    };
    for y in 0..th {
        let (y0, y1, fy) = axis(y, h, th);
        for x in 0..tw {
            let (x0, x1, fx) = axis(x, w, tw);
            let px = |xx: usize, yy: usize, c: usize| src[(yy * w as usize + xx) * 4 + c] as f32;
            for c in 0..4 {
                let top = px(x0, y0, c) * (1.0 - fx) + px(x1, y0, c) * fx;
                let bot = px(x0, y1, c) * (1.0 - fx) + px(x1, y1, c) * fx;
                out[((y * tw + x) * 4) as usize + c] = (top * (1.0 - fy) + bot * fy).round() as u8;
            }
        }
    }
    out
}

/// Registered fonts of a host: index = `GetFontIndex` result.
#[derive(Default)]
pub struct FontTable {
    pub entries: Vec<(String, Option<Arc<FontAtlas>>)>,
}

#[cfg(test)]
mod tests {

    /// A picture saved with a situation goes up as the script's own would (#1559).
    #[test]
    fn a_saved_picture_goes_up_once_restored() {
        let mut t = ScriptTexture::new(2, 1);
        let _ = t.take_upload();
        assert!(!t.restore(3, 1, vec![0; 12]), "another size is not taken");
        assert!(t.restore(2, 1, vec![9; 8]));
        assert_eq!(t.take_upload(), Some(vec![9; 8]));
    }
    use super::ScriptTexture;

    /// A picture fed from another player's game goes up, and what the scripts draw here
    /// afterwards does not; the same picture fed again goes up no more.
    #[test]
    fn a_fed_picture_shows_instead_of_the_scripts_own_drawing() {
        let mut t = ScriptTexture::new(2, 1);
        let _ = t.take_upload();
        t.feed(2, 1, vec![7; 8]);
        assert_eq!(t.take_upload(), Some(vec![7; 8]));
        t.feed(2, 1, vec![7; 8]);
        assert_eq!(t.take_upload(), None, "the same picture again");
        t.color = [255; 4];
        t.rect(0, 0, 1, 0);
        t.unlock();
        assert_eq!(t.take_upload(), None, "the script's drawing stays at home");
        t.feed(2, 1, vec![8; 8]);
        assert_eq!(t.take_upload(), Some(vec![8; 8]));
        assert_eq!(t.get(1, 0), [8; 4]);
    }

    /// A plugin's picture (`AccessScriptTexture`) goes up at the texture's size only, and
    /// while a script holds the texture locked it waits as the released image.
    #[test]
    fn a_plugins_picture_replaces_the_texture() {
        let mut t = ScriptTexture::new(2, 1);
        let _ = t.take_upload();
        assert!(!t.replace(1, 2, &[1; 8]), "another size is not taken");
        assert!(!t.replace(2, 1, &[1; 4]), "nor a short buffer");
        assert!(t.replace(2, 1, &[1; 8]));
        assert_eq!(t.take_upload(), Some(vec![1; 8]));
        assert_eq!(t.take_upload(), None);
        // locked by a script that is drawing: its buffer stays, the plugin's goes up
        t.locked = true;
        t.put(0, 0, [5; 4]);
        assert!(t.replace(2, 1, &[2; 8]));
        assert_eq!(t.take_upload(), Some(vec![2; 8]));
        assert_eq!(t.get(0, 0), [5; 4]);
    }

    #[test]
    fn a_loaded_bitmap_fills_the_texture_at_its_own_power_of_two_size() {
        let mut t = ScriptTexture::new(4, 2);
        // 3x1, white red blue: 4x1, stretched across
        let src = [255, 255, 255, 255, 255, 0, 0, 255, 0, 0, 255, 255];
        t.load(3, 1, &src);
        assert_eq!((t.width, t.height), (4, 1));
        assert!(t.mipmaps);
        assert_eq!(t.get(0, 0), [255, 255, 255, 255]);
        assert_eq!(t.get(3, 0), [0, 0, 255, 255]);
        // a power-of-two bitmap is taken as it is
        let px: Vec<u8> = (0..8 * 2 * 4).map(|i| i as u8).collect();
        t.load(8, 2, &px);
        assert_eq!((t.width, t.height, t.rgba.as_slice()), (8, 2, px.as_slice()));
        // STNewTex: back to the declared size, empty
        t.color = [255; 4];
        t.renew();
        assert_eq!((t.width, t.height), (4, 2));
        assert!(t.rgba.iter().all(|b| *b == 0));
        assert_eq!(t.color, [0; 4]);
    }

    #[test]
    fn locked_edits_wait_for_unlock_and_latest_release_wins() {
        let mut t = ScriptTexture::new(1, 1);
        t.locked = true;
        t.put(0, 0, [1; 4]);
        assert!(t.take_upload().is_none());
        t.unlock();
        t.locked = true;
        t.put(0, 0, [2; 4]);
        t.unlock();
        t.locked = true;
        // A distant vehicle can release several pictures before its next upload.
        assert_eq!(t.take_upload().unwrap(), vec![2; 4]);
        assert!(t.take_upload().is_none());
    }

    #[test]
    fn unlocked_drawing_still_uploads_without_st_callbacks() {
        let mut t = ScriptTexture::new(1, 1);
        t.put(0, 0, [1; 4]);
        t.unlock();
        t.put(0, 0, [2; 4]);
        assert_eq!(t.take_upload().unwrap(), vec![2; 4]);
        assert!(t.take_upload().is_none());
    }
}
