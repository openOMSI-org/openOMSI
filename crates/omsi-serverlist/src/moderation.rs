//! What may be shown in the public server list: a server's name, its message of the day and
//! its icon, checked by one `Moderator` in two places - on the server before it registers
//! (a flagged name is refused, a flagged icon keeps it out of the list) and on every client
//! before it shows a listing (a flagged text is covered, an icon that was not cleared is the
//! default one). The list service checks too, but no client takes its word for it.
//!
//! **Text** is checked by `BuiltinModerator`: the `rustrict` filter (MIT; profanity, slurs and
//! the usual ways of writing around a filter, no word list to keep) plus a few rules of our
//! own for the list (no addresses, no hidden or control characters, a length).
//!
//! **Icons** are checked by an `ImageClassifier`. None is built in: OpenLB, the library the
//! feature was planned around, cannot be shipped (see `docs/SERVER.md`, "Public servers":
//! its licence limits what the software may be used for, its models have no licence at all,
//! and it needs a native ONNX runtime library on every platform), and no classifier of an
//! open licence is bundled yet. So by default **no custom icon is ever cleared**: a listing
//! keeps its icon on the server's own status page, and the public list shows the default one.
//! A classifier (a model run by `tract` or `ort`, downloaded to the content folder with a
//! checksum) plugs in with `set_image_classifier` and nothing else changes.

use std::sync::{Arc, OnceLock, RwLock};

/// What a moderator says of a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Clean,
    /// Not to be shown; the reason, for the host (never shown to other players).
    Flagged(&'static str),
}

impl Verdict {
    pub fn is_clean(&self) -> bool {
        *self == Verdict::Clean
    }
}

/// What a moderator says of an icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconVerdict {
    /// Looked at and fine: may be shown.
    Cleared,
    /// Looked at and not fine: the listing is refused until the icon is changed.
    Flagged(&'static str),
    /// Could not be looked at (no classifier, or it is not ready yet): treated as not shown.
    Unchecked,
}

/// The one thing the public list asks of a moderation: text and icons in, a verdict out.
pub trait Moderator: Send + Sync {
    fn check_text(&self, text: &str) -> Verdict;
    fn check_icon(&self, png: &[u8]) -> IconVerdict;
}

/// A model that scores a picture (0 = fine, 1 = certainly not). Plugged in with
/// `set_image_classifier`.
pub trait ImageClassifier: Send + Sync {
    /// The score of the decoded 64x64 RGBA picture, or None when the model is not ready.
    fn score(&self, rgba: &[u8], width: u32, height: u32) -> Option<f32>;
}

/// At or above this score an icon is flagged.
pub const ICON_THRESHOLD: f32 = 0.35;
/// The largest side of an icon the list takes, and its file size.
pub const MAX_ICON_SIDE: u32 = 128;
pub const MAX_ICON_BYTES: usize = 256 * 1024;
/// The longest name and message the list takes (characters).
pub const MAX_NAME: usize = 48;
pub const MAX_MOTD: usize = 120;

/// The size of a PNG's picture from its header, without decoding it.
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || !png.starts_with(b"\x89PNG\r\n\x1a\n") || &png[12..16] != b"IHDR" {
        return None;
    }
    Some((
        u32::from_be_bytes(png[16..20].try_into().ok()?),
        u32::from_be_bytes(png[20..24].try_into().ok()?),
    ))
}

static CLASSIFIER: OnceLock<RwLock<Option<Arc<dyn ImageClassifier>>>> = OnceLock::new();

/// Plug in the icon classifier of this process (replacing an earlier one).
pub fn set_image_classifier(c: Arc<dyn ImageClassifier>) {
    *CLASSIFIER
        .get_or_init(Default::default)
        .write()
        .unwrap_or_else(|e| e.into_inner()) = Some(c);
}

fn classifier() -> Option<Arc<dyn ImageClassifier>> {
    CLASSIFIER
        .get()?
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// The moderation the game uses: `rustrict` and our rules for text, the plugged-in classifier
/// (if any) for icons.
#[derive(Default, Clone, Copy)]
pub struct BuiltinModerator;

impl Moderator for BuiltinModerator {
    fn check_text(&self, text: &str) -> Verdict {
        use rustrict::CensorStr;
        let t = text.trim();
        if t.chars().any(|c| c.is_control() || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2064}' | '\u{feff}')) {
            return Verdict::Flagged("hidden or control characters");
        }
        let low = t.to_lowercase();
        if ["http:", "https:", "www.", "discord.gg", "t.me/"]
            .iter()
            .any(|m| low.contains(m))
        {
            return Verdict::Flagged("an address");
        }
        if t.is(rustrict::Type::INAPPROPRIATE) {
            return Verdict::Flagged("inappropriate language");
        }
        Verdict::Clean
    }

    fn check_icon(&self, png: &[u8]) -> IconVerdict {
        icon_verdict(classifier().as_deref(), png)
    }
}

/// An icon's verdict with this classifier (None: none plugged in).
pub fn icon_verdict(classifier: Option<&dyn ImageClassifier>, png: &[u8]) -> IconVerdict {
    // (a picture that is not a small PNG is never shown, whatever the classifier thinks)
    let Some((w, h)) = png_size(png) else {
        return IconVerdict::Flagged("not a PNG");
    };
    if png.len() > MAX_ICON_BYTES || w == 0 || h == 0 || w > MAX_ICON_SIDE || h > MAX_ICON_SIDE {
        return IconVerdict::Flagged("too big (at most 128x128 and 256 KB)");
    }
    let Some(c) = classifier else {
        return IconVerdict::Unchecked;
    };
    let Some(rgba) = decode_rgba(png) else {
        return IconVerdict::Flagged("not a readable PNG");
    };
    match c.score(&rgba.0, rgba.1, rgba.2) {
        Some(s) if s >= ICON_THRESHOLD => IconVerdict::Flagged("flagged by the image check"),
        Some(s) if s.is_finite() => IconVerdict::Cleared,
        _ => IconVerdict::Unchecked,
    }
}

fn decode_rgba(png: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

/// A listing as a client shows it: what the list service sent, after the client's own check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub name: String,
    pub motd: String,
    /// The icon may be shown (cleared); else the default one.
    pub icon_ok: bool,
    /// Name or message was covered.
    pub covered: bool,
}

/// A text as the list keeps it: no control characters, runs of spaces as one, at most `max`
/// characters.
pub fn clean(text: &str, max: usize) -> String {
    let spaced: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    spaced
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// What stands in the place of a covered text.
pub const COVERED: &str = "[hidden]";

/// A text cleaned for the list and cut to `max` characters, or covered if it is flagged.
pub fn show_text(m: &dyn Moderator, text: &str, max: usize) -> (String, bool) {
    let t = clean(text, max);
    if m.check_text(&t).is_clean() {
        (t, false)
    } else {
        (COVERED.to_string(), true)
    }
}

/// A listing for display: the name and message cleaned and covered if flagged; the icon is
/// shown only if this client cleared it itself (`icon` is the PNG the service sent, or None).
pub fn show_listing(m: &dyn Moderator, name: &str, motd: &str, icon: Option<&[u8]>) -> Shown {
    let (name, c1) = show_text(m, name, MAX_NAME);
    let (motd, c2) = show_text(m, motd, MAX_MOTD);
    let icon_ok = icon.is_some_and(|i| m.check_icon(i) == IconVerdict::Cleared);
    Shown {
        name,
        motd,
        icon_ok,
        covered: c1 || c2,
    }
}

/// Why a host may not list itself, if it may not: its name or message is flagged, or its icon
/// is. (An icon that cannot be checked does not stop the listing: it goes without an icon.)
pub fn listing_refusal(
    m: &dyn Moderator,
    name: &str,
    motd: &str,
    icon: Option<&[u8]>,
) -> Option<String> {
    if name.trim().is_empty() {
        return Some("the server has no name".into());
    }
    if let Verdict::Flagged(why) = m.check_text(name) {
        return Some(format!(
            "the server's name is not allowed in the public list ({why}): change `name` in server.cfg"
        ));
    }
    if let Verdict::Flagged(why) = m.check_text(motd) {
        return Some(format!(
            "the server's message is not allowed in the public list ({why}): change `motd` in server.cfg"
        ));
    }
    if let Some(IconVerdict::Flagged(why)) = icon.filter(|i| !i.is_empty()).map(|i| m.check_icon(i))
    {
        return Some(format!(
            "the server's icon is not allowed in the public list ({why}): change server-icon.png"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A moderator for the tests: flags the text "FLAGGED" and icons whose first byte after
    /// the header says so.
    struct Double;

    impl Moderator for Double {
        fn check_text(&self, text: &str) -> Verdict {
            if text.contains("FLAGGED") {
                Verdict::Flagged("test")
            } else {
                Verdict::Clean
            }
        }
        fn check_icon(&self, png: &[u8]) -> IconVerdict {
            match png.last() {
                Some(b'F') => IconVerdict::Flagged("test"),
                Some(b'C') => IconVerdict::Cleared,
                _ => IconVerdict::Unchecked,
            }
        }
    }

    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v
    }

    #[test]
    fn a_registration_is_refused_for_a_flagged_name_message_or_icon() {
        let m = Double;
        assert_eq!(listing_refusal(&m, "Fine server", "Welcome", None), None);
        assert!(
            listing_refusal(&m, "A FLAGGED name", "x", None)
                .unwrap()
                .contains("name")
        );
        assert!(
            listing_refusal(&m, "Fine", "FLAGGED", None)
                .unwrap()
                .contains("message")
        );
        assert!(listing_refusal(&m, "  ", "", None).is_some());
        assert!(
            listing_refusal(&m, "Fine", "", Some(b"xF"))
                .unwrap()
                .contains("icon")
        );
        // an icon nobody could check does not stop the listing (it goes without one)
        assert_eq!(listing_refusal(&m, "Fine", "", Some(b"x?")), None);
        assert_eq!(listing_refusal(&m, "Fine", "", Some(b"xC")), None);
    }

    #[test]
    fn a_client_covers_flagged_text_and_shows_only_an_icon_it_cleared() {
        let m = Double;
        let s = show_listing(&m, "Nice\nbus  server", "FLAGGED words", Some(b"xC"));
        assert_eq!(
            s,
            Shown {
                name: "Nice bus server".into(),
                motd: COVERED.into(),
                icon_ok: true,
                covered: true
            }
        );
        assert!(!show_listing(&m, "ok", "ok", Some(b"xF")).icon_ok);
        assert!(
            !show_listing(&m, "ok", "ok", Some(b"x?")).icon_ok,
            "unchecked is not shown"
        );
        assert!(!show_listing(&m, "ok", "ok", None).icon_ok);
        // too long is cut, not refused
        assert_eq!(
            show_listing(&m, &"n".repeat(200), "", None)
                .name
                .chars()
                .count(),
            MAX_NAME
        );
    }

    #[test]
    fn the_builtin_rules_for_the_list() {
        let m = BuiltinModerator;
        for fine in [
            "Berlin Spandau 24/7",
            "Bus Simulator Brasil",
            "Welcome! Drive safely.",
            "Linha 100 - Bahnhof",
        ] {
            assert_eq!(m.check_text(fine), Verdict::Clean, "{fine}");
        }
        for (bad, why) in [
            ("join www.example.com", "an address"),
            ("https://x.example", "an address"),
            ("a\u{202e}b", "hidden or control characters"),
            ("tab\there", "hidden or control characters"),
        ] {
            assert_eq!(m.check_text(bad), Verdict::Flagged(why), "{bad}");
        }
        // a word the filter's own tests carry
        assert!(!m.check_text("damn").is_clean() || !m.check_text("shit").is_clean());
    }

    /// A classifier that scores by the mean red of the picture, to try the plumbing: the
    /// real one is a model.
    struct Reddish(Option<f32>);

    impl ImageClassifier for Reddish {
        fn score(&self, rgba: &[u8], w: u32, h: u32) -> Option<f32> {
            assert_eq!(rgba.len(), (w * h * 4) as usize);
            self.0.or_else(|| {
                Some(
                    rgba.chunks(4).map(|p| p[0] as f32).sum::<f32>()
                        / (rgba.len() / 4) as f32
                        / 255.0,
                )
            })
        }
    }

    fn png_of(rgb: [u8; 3]) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_classifier_clears_or_flags_a_decoded_icon_and_not_ready_is_unchecked() {
        let (blue, red) = (png_of([10, 10, 200]), png_of([250, 10, 10]));
        assert_eq!(
            icon_verdict(Some(&Reddish(None)), &blue),
            IconVerdict::Cleared
        );
        assert!(matches!(
            icon_verdict(Some(&Reddish(None)), &red),
            IconVerdict::Flagged(_)
        ));
        assert_eq!(
            icon_verdict(Some(&Reddish(Some(f32::NAN))), &blue),
            IconVerdict::Unchecked
        );
        assert_eq!(icon_verdict(None, &blue), IconVerdict::Unchecked);
        // what is not a picture is flagged whatever the classifier says
        let mut broken = png_header(8, 8);
        broken.extend_from_slice(b"garbage");
        assert!(matches!(
            icon_verdict(Some(&Reddish(Some(0.0))), &broken),
            IconVerdict::Flagged(_)
        ));
    }

    #[test]
    fn an_icon_is_never_cleared_without_a_classifier() {
        let m = BuiltinModerator;
        assert_eq!(m.check_icon(&png_header(64, 64)), IconVerdict::Unchecked);
        assert!(matches!(
            m.check_icon(b"GIF89a....................."),
            IconVerdict::Flagged(_)
        ));
        assert!(matches!(
            m.check_icon(&png_header(5000, 5000)),
            IconVerdict::Flagged(_)
        ));
        assert!(matches!(
            m.check_icon(&png_header(0, 64)),
            IconVerdict::Flagged(_)
        ));
        assert_eq!(png_size(&png_header(64, 32)), Some((64, 32)));
        assert_eq!(png_size(b"nope"), None);
    }
}
