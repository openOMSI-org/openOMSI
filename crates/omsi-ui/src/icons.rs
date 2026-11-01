//! Built-in Material Symbols and custom SVG icons from `assets/icons`.

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

/// The SVG source of an icon by name (`directions_bus`, `stop_request`).
pub fn svg(name: &str) -> Option<&'static str> {
    ICONS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// Every icon name there is.
pub fn names() -> impl Iterator<Item = &'static str> {
    ICONS.iter().map(|(n, _)| *n)
}

/// An SVG read as a tree. An `<image>` is taken only from inside the SVG (a `data:` URL): a
/// theme's logo comes from a mod, and a file name or path in it would let it read any file
/// on the player's machine (`/dev/zero` forever), not only its theme's folder.
fn tree(src: &str) -> Option<resvg::usvg::Tree> {
    use resvg::usvg::{ImageHrefResolver, Options};
    let opt = Options {
        image_href_resolver: ImageHrefResolver {
            resolve_data: ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        ..Options::default()
    };
    resvg::usvg::Tree::from_str(src, &opt).ok()
}

/// An icon as an anti-aliased alpha mask of `size` x `size` pixels, with the SVG's margins.
pub fn rasterize(name: &str, size: u32) -> Option<Vec<u8>> {
    let src = svg(name)?;
    let tree = tree(src)?;
    let mut pix = resvg::tiny_skia::Pixmap::new(size, size)?;
    let s = tree.size();
    let k = size as f32 / s.width().max(s.height());
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pix.as_mut());
    Some(pix.pixels().iter().map(|p| p.alpha()).collect())
}

/// Any SVG (a logo) in its own colours, fitted into `w` x `h` pixels and centred: straight
/// (not premultiplied) RGBA, as the interface's textures are.
pub fn rasterize_rgba(src: &str, w: u32, h: u32) -> Option<Vec<u8>> {
    let tree = tree(src)?;
    let mut pix = resvg::tiny_skia::Pixmap::new(w, h)?;
    let s = tree.size();
    let k = (w as f32 / s.width()).min(h as f32 / s.height());
    let (dx, dy) = ((w as f32 - s.width() * k) * 0.5, (h as f32 - s.height() * k) * 0.5);
    resvg::render(&tree, resvg::tiny_skia::Transform::from_row(k, 0.0, 0.0, k, dx, dy), &mut pix.as_mut());
    Some(pix.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }).collect())
}

/// The size an SVG is drawn at (its width and height in its own units); None when it cannot
/// be read.
pub fn svg_size(src: &str) -> Option<(f32, f32)> {
    let tree = tree(src)?;
    let s = tree.size();
    Some((s.width(), s.height()))
}

/// The length of the path `id` of an SVG (in its own units), as far as its stroke goes:
/// what a theme's loading logo is drawn along (`progress_svg`). None when there is no such path.
pub fn path_length(src: &str, id: &str) -> Option<f32> {
    use resvg::tiny_skia::PathSegment;
    let tree = tree(src)?;
    let resvg::usvg::Node::Path(path) = tree.node_by_id(id)? else { return None };
    let (mut len, mut at, mut start) = (0.0f32, (0.0f32, 0.0f32), (0.0f32, 0.0f32));
    let dist = |a: (f32, f32), b: (f32, f32)| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    // (a curve in 32 straight pieces: a tenth of a per cent off for the arcs of a logo)
    let mut curve = |at: &mut (f32, f32), f: &dyn Fn(f32) -> (f32, f32)| {
        for k in 1..=32 {
            let p = f(k as f32 / 32.0);
            len += dist(*at, p);
            *at = p;
        }
    };
    for seg in path.data().segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                at = (p.x, p.y);
                start = at;
            }
            PathSegment::LineTo(p) => {
                curve(&mut at, &|_| (p.x, p.y));
            }
            PathSegment::QuadTo(c, p) => {
                let a = at;
                curve(&mut at, &|t| {
                    let u = 1.0 - t;
                    (u * u * a.0 + 2.0 * u * t * c.x + t * t * p.x, u * u * a.1 + 2.0 * u * t * c.y + t * t * p.y)
                });
            }
            PathSegment::CubicTo(c1, c2, p) => {
                let a = at;
                curve(&mut at, &|t| {
                    let u = 1.0 - t;
                    let (k0, k1, k2, k3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    (k0 * a.0 + k1 * c1.x + k2 * c2.x + k3 * p.x, k0 * a.1 + k1 * c1.y + k2 * c2.y + k3 * p.y)
                });
            }
            PathSegment::Close => {
                let s = start;
                curve(&mut at, &|_| s);
            }
        }
    }
    (len > 0.0).then_some(len)
}

/// The SVG `src` with its path `id` (of length `len`, `path_length`) drawn only along its first
/// `frac` (0 to 1) - a dash as long as that, the rest a gap - and, when `track` is above 0, the
/// whole of it once more under that at the opacity `track`: a loading bar in the shape of a
/// logo. The path must be one element closed in itself (`<path ... />`) without a dash of its own.
pub fn progress_svg(src: &str, id: &str, len: f32, frac: f32, track: f32) -> String {
    let Some(at) = [format!("id=\"{id}\""), format!("id='{id}'")].iter().find_map(|a| src.find(a.as_str()).map(|i| (i, a.len()))) else {
        return src.to_string();
    };
    let frac = frac.clamp(0.0, 1.0);
    let dash = if frac <= 0.0005 { " stroke-opacity=\"0\"".to_string() } else { format!(" stroke-dasharray=\"{:.3} {:.3}\"", len * frac, len * 2.0) };
    let mut out = String::with_capacity(src.len() + 256);
    let open = src[..at.0].rfind('<');
    let close = src[at.0..].find("/>").map(|k| at.0 + k + 2);
    match (open, close) {
        (Some(o), Some(c)) if track > 0.0 && !src[at.0..c].contains('<') => {
            // the track: the element as it is, faint, the dashed one after it
            let element = &src[o..c];
            let id_in = at.0 - o;
            out.push_str(&src[..o]);
            out.push_str(&element[..id_in]);
            out.push_str(&format!("stroke-opacity=\"{track:.3}\""));
            out.push_str(&element[id_in + at.1..]);
            out.push_str(&element[..id_in + at.1]);
            out.push_str(&dash);
            out.push_str(&element[id_in + at.1..]);
            out.push_str(&src[c..]);
        }
        _ => {
            out.push_str(&src[..at.0 + at.1]);
            out.push_str(&dash);
            out.push_str(&src[at.0 + at.1..]);
        }
    }
    out
}

/// An element of a loading logo that comes in when the loading reaches it: a top-level element
/// of the SVG with an `id` and `data-at="0.4"` (how far along the progress, 0 to 1). It pops in
/// - or, with `data-blink`, flashes twice, a light - `data-delay` seconds after that.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub id: String,
    pub at: f32,
    pub delay: f32,
    pub blink: bool,
    /// The middle of the element (in the SVG's units): what it grows from.
    pub center: (f32, f32),
}

/// How a mark is drawn now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    Hidden,
    /// Growing (or shrinking back from a little more than its size, 0 to about 1.1).
    Scale(f32),
    Shown,
}

impl Stage {
    /// The stage of `mark` `t` seconds after the loading reached it (negative: not yet): popping
    /// in over 0.35 s with a little overshoot, or two flashes of 0.14 s.
    pub fn of(mark: &Mark, t: Option<f32>) -> Stage {
        let Some(t) = t.map(|t| t - mark.delay) else { return Stage::Hidden };
        if t < 0.0 {
            return Stage::Hidden;
        }
        if mark.blink {
            return if t < 0.56 && t % 0.28 < 0.14 { Stage::Shown } else { Stage::Hidden };
        }
        const POP: f32 = 0.35;
        if t >= POP {
            return Stage::Shown;
        }
        // (ease out with a little overshoot, as a bubble that pops up)
        let x = t / POP - 1.0;
        let (c1, c3) = (1.70158f32, 2.70158f32);
        Stage::Scale((1.0 + c3 * x * x * x + c1 * x * x).max(0.0))
    }
}

/// The marks of an SVG (`data-at`), in the order they stand in it.
pub fn marks(src: &str) -> Vec<Mark> {
    let Some(tree) = tree(src) else { return Vec::new() };
    let attr = |tag: &str, name: &str| -> Option<String> {
        let k = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        Some(tag[k..k + tag[k..].find('"')?].to_string())
    };
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(k) = src[from..].find("data-at=\"") {
        let at_attr = from + k;
        from = at_attr + 9;
        let Some(open) = src[..at_attr].rfind('<') else { continue };
        let Some(end) = src[at_attr..].find('>') else { continue };
        let tag = &src[open..at_attr + end];
        let (Some(id), Some(at)) = (attr(tag, "id"), attr(tag, "data-at").and_then(|a| a.parse::<f32>().ok())) else { continue };
        let Some(node) = tree.node_by_id(&id) else { continue };
        let b = node.abs_bounding_box();
        // (the box is the canvas's, the view box applied: back into the SVG's own units, where
        // the element is grown round its middle)
        // (the element's place on the canvas is its parent's transform after its own: the
        // parent's, undone, gives the middle where the wrapper round it stands)
        let mut c = resvg::tiny_skia::Point::from_xy(b.x() + b.width() * 0.5, b.y() + b.height() * 0.5);
        let own = match node {
            resvg::usvg::Node::Group(g) => g.transform(),
            _ => resvg::tiny_skia::Transform::identity(),
        };
        if let Some(back) = own.invert().and_then(|undo| node.abs_transform().pre_concat(undo).invert()) {
            back.map_point(&mut c);
        }
        out.push(Mark {
            id,
            at: at.clamp(0.0, 1.0),
            delay: attr(tag, "data-delay").and_then(|d| d.parse::<f32>().ok()).unwrap_or(0.0).clamp(0.0, 10.0),
            blink: attr(tag, "data-blink").is_some(),
            center: (c.x, c.y),
        });
    }
    out
}

/// Where the element `id` lies in `src`: from its `<` to the end of its closing tag.
fn element_span(src: &str, id: &str) -> Option<(usize, usize)> {
    let at = src.find(&format!(" id=\"{id}\""))?;
    let open = src[..at].rfind('<')?;
    let name: String = src[open + 1..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == ':').collect();
    let gt = at + src[at..].find('>')?;
    if src[..gt].ends_with('/') {
        return Some((open, gt + 1));
    }
    // (the same element nested inside it: counted)
    let (opening, closing) = (format!("<{name}"), format!("</{name}>"));
    let (mut depth, mut i) = (1, gt + 1);
    while depth > 0 {
        let next_close = i + src[i..].find(&closing)?;
        let next_open = src[i..next_close].match_indices(&opening).map(|(k, _)| i + k).find(|&k| src[k + opening.len()..].starts_with([' ', '>', '/']));
        match next_open {
            Some(k) => {
                let tag_end = k + src[k..].find('>')?;
                if !src[..tag_end].ends_with('/') {
                    depth += 1;
                }
                i = tag_end + 1;
            }
            None => {
                depth -= 1;
                i = next_close + closing.len();
            }
        }
    }
    Some((open, i))
}

/// The SVG `src` with its marks drawn as `stage` says: hidden, growing from their middle, or as
/// they are.
pub fn staged_svg(src: &str, marks: &[Mark], stage: impl Fn(&Mark) -> Stage) -> String {
    let mut out = src.to_string();
    for m in marks {
        let wrap = match stage(m) {
            Stage::Shown => continue,
            Stage::Hidden => "<g opacity=\"0\">".to_string(),
            Stage::Scale(s) => {
                let (cx, cy) = m.center;
                format!("<g transform=\"translate({cx:.2} {cy:.2}) scale({s:.3}) translate({:.2} {:.2})\">", -cx, -cy)
            }
        };
        if let Some((a, b)) = element_span(&out, &m.id) {
            out = format!("{}{wrap}{}</g>{}", &out[..a], &out[a..b], &out[b..]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    /// A logo whose stops come in as the loading reaches them, and a light that flashes.
    #[test]
    fn marks_come_in_when_the_loading_reaches_them() {
        let src = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100"><g id="stop" data-at="0.5" transform="translate(10 0)"><circle cx="90" cy="50" r="20" fill="#fff"/><g><rect x="85" y="45" width="10" height="10" fill="#f00"/></g></g><circle id="lamp" data-at="0.2" data-delay="0.35" data-blink="" cx="20" cy="20" r="5" fill="#ff0"/></svg>"##;
        let m = super::marks(src);
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].id.as_str(), m[0].at, m[0].blink, m[0].center), ("stop", 0.5, false, (100.0, 50.0)));
        assert_eq!((m[1].id.as_str(), m[1].delay, m[1].blink), ("lamp", 0.35, true));
        // popping in: from nothing, a little over its size, then as it is
        use super::Stage;
        assert_eq!(Stage::of(&m[0], None), Stage::Hidden);
        assert_eq!(Stage::of(&m[0], Some(0.0)), Stage::Scale(0.0));
        let Stage::Scale(over) = Stage::of(&m[0], Some(0.25)) else { panic!() };
        assert!(over > 1.0 && over < 1.15, "{over}");
        assert_eq!(Stage::of(&m[0], Some(1.0)), Stage::Shown);
        // the light: after its delay, two flashes, then dark
        let lamp = |t: f32| Stage::of(&m[1], Some(t));
        assert_eq!([lamp(0.1), lamp(0.4), lamp(0.55), lamp(0.68), lamp(0.83), lamp(2.0)], [Stage::Hidden, Stage::Shown, Stage::Hidden, Stage::Shown, Stage::Hidden, Stage::Hidden]);
        // drawn: the whole group hidden or grown, nested groups and all
        let ink = |svg: &str| super::rasterize_rgba(svg, 200, 100).unwrap().chunks(4).filter(|p| p[3] > 128).count();
        let all = ink(&super::staged_svg(src, &m, |_| Stage::Shown));
        let none = ink(&super::staged_svg(src, &m, |_| Stage::Hidden));
        let half = ink(&super::staged_svg(src, &m, |_| Stage::Scale(0.5)));
        assert_eq!(none, 0);
        assert!(half > all / 6 && half < all / 3, "{half} of {all}");
        assert!(super::staged_svg(src, &m, |_| Stage::Hidden).matches("</g>").count() == 4);
        // (a view box that does not start at 0, as a logo cut to its ink has it: the middle in
        // the SVG's own units, so that a mark grows where it stands)
        let moved = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="96 54 200 100"><g id="stop" data-at="0.5" transform="translate(-20 0)"><circle cx="210" cy="104" r="20" fill="#fff"/></g></svg>"##;
        let m = super::marks(moved);
        let (cx, cy) = m[0].center;
        assert!((cx - 190.0).abs() < 0.01 && (cy - 104.0).abs() < 0.01, "{:?}", m[0].center);
        let ink_at = |svg: &str| {
            let px = super::rasterize_rgba(svg, 200, 100).unwrap();
            let (n, sx, sy) = px.chunks(4).enumerate().filter(|(_, p)| p[3] > 128).fold((0.0f32, 0.0f32, 0.0f32), |(n, x, y), (i, _)| (n + 1.0, x + (i % 200) as f32, y + (i / 200) as f32));
            (sx / n, sy / n)
        };
        let (whole, half) = (ink_at(moved), ink_at(&super::staged_svg(moved, &m, |_| Stage::Scale(0.5))));
        assert!((whole.0 - half.0).abs() < 1.0 && (whole.1 - half.1).abs() < 1.0, "{whole:?} {half:?}");
    }

    /// A ring round a box and a line under a word, as a logo has it: its length, and drawn
    /// along a part of it.
    const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 200"><rect x="60" y="60" width="80" height="80" fill="#222"/><path id="progress" d="M100 20 A80 80 0 1 0 180 100 L380 100" fill="none" stroke="#2a75f7" stroke-width="16" stroke-linecap="round"/></svg>"##;

    #[test]
    fn a_logo_is_a_loading_bar_along_its_path() {
        // three quarters of a circle of radius 80 and a line of 200
        let len = super::path_length(LOGO, "progress").unwrap();
        let want = 2.0 * std::f32::consts::PI * 80.0 * 0.75 + 200.0;
        assert!((len - want).abs() < want * 0.002, "{len} vs {want}");
        assert!(super::path_length(LOGO, "nothing").is_none());
        let blue = |src: &str| {
            let px = super::rasterize_rgba(src, 400, 200).unwrap();
            // (the end of the line, right; the start of the ring, top)
            let at = |x: usize, y: usize| px[(y * 400 + x) * 4..(y * 400 + x) * 4 + 4].to_vec();
            (at(370, 100), at(100, 20))
        };
        // nothing drawn yet, a faint track
        let (end, start) = blue(&super::progress_svg(LOGO, "progress", len, 0.0, 0.2));
        assert!(end[3] > 20 && end[3] < 90, "{end:?}");
        assert!(start[3] < 90, "{start:?}");
        // half: the ring drawn, the line's end not yet
        let (end, start) = blue(&super::progress_svg(LOGO, "progress", len, 0.5, 0.2));
        assert!(start[3] > 200 && start[2] > 200, "{start:?}");
        assert!(end[3] < 90, "{end:?}");
        // all of it
        let (end, _) = blue(&super::progress_svg(LOGO, "progress", len, 1.0, 0.0));
        assert!(end[3] > 200, "{end:?}");
        // (without a track the element is not doubled; an SVG without the id stays as it is)
        assert_eq!(super::progress_svg(LOGO, "progress", len, 1.0, 0.0).matches("<path").count(), 1);
        assert_eq!(super::progress_svg(LOGO, "progress", len, 1.0, 0.2).matches("<path").count(), 2);
        assert_eq!(super::progress_svg(LOGO, "nothing", len, 0.5, 0.2), LOGO);
    }

    #[test]
    fn stop_request_has_smooth_edges_and_an_open_background() {
        for size in [24, 32, 64] {
            let alpha = super::rasterize("stop_request", size).unwrap();
            assert!(alpha.iter().any(|&a| a > 0 && a < 255), "edges must be anti-aliased");
            assert_eq!(alpha[(size / 4 * size + size / 2) as usize], 0);
            assert_eq!(alpha[(size / 2 * size + size * 3 / 4) as usize], 0, "the bar's interior is transparent");
            assert!(alpha.iter().filter(|&&a| a > 128).count() > (size * size / 12) as usize);
        }
    }

    #[test]
    fn icons_are_built_in_and_draw() {
        assert!(super::names().count() > 50);
        let a = super::rasterize("directions_bus", 32).unwrap();
        assert_eq!(a.len(), 32 * 32);
        let covered = a.iter().filter(|&&v| v > 128).count();
        assert!(covered > 150 && covered < 900, "{covered}");
        assert!(super::rasterize("no_such_icon", 32).is_none());
    }

    /// An `<image>` of a logo that names a file outside it is left out (usvg's own options
    /// would read any file); one inside it (`data:`) is drawn.
    #[test]
    fn a_logo_reads_no_file_of_its_own_accord() {
        fn images(g: &resvg::usvg::Group) -> usize {
            g.children().iter().map(|n| match n {
                resvg::usvg::Node::Image(_) => 1,
                resvg::usvg::Node::Group(g) => images(g),
                _ => 0,
            }).sum()
        }
        let outside = std::env::temp_dir().join(format!("omsi-ui-outside-{}.svg", std::process::id()));
        std::fs::write(&outside, r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#).unwrap();
        let href = outside.to_string_lossy().replace('\\', "/");
        let src = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><rect width="5" height="5"/><image href="{href}" width="10" height="10"/></svg>"#);
        let loose = resvg::usvg::Tree::from_str(&src, &resvg::usvg::Options::default()).unwrap();
        assert_eq!(images(loose.root()), 1, "usvg's own options read the file: the test reaches it");
        assert_eq!(images(super::tree(&src).unwrap().root()), 0);
        assert!(super::svg_size(&src).is_some());
        std::fs::remove_file(&outside).ok();

        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let src = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><image href="data:image/png;base64,{png}" width="10" height="10"/></svg>"#);
        assert_eq!(images(super::tree(&src).unwrap().root()), 1);
    }
}
