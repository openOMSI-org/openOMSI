//! `*.tchof.xml` HOF templates: what each display string of a depot file is (#732).
//!
//! A depot file only holds strings by index - the vehicle's scripts read them with
//! `GetTerminusString`, so nothing in the `.hof` says that string 3 is the front sign's first
//! line and string 17 the wide interior display's. The packs that ship more than one kind of
//! display (BKK-FUTÁR, Vultron, FOK-GYEM, Annax, the IBIS boxes) ship a template beside the
//! depot files that names every slot and says how long its text may be and whether it is
//! written in capitals:
//!
//! ```xml
//! <HOFTemplate>
//!     <TemplateName>The Creators HOF</TemplateName>
//!     <TemplateDesc>...</TemplateDesc>
//!     <TerminusStrings>
//!         <String MaxLength="16" Characters="UPPER">IBIS Display</String>
//!         <String MaxLength="" Characters="NORMAL">Annax Front (1st row)</String>
//!     </TerminusStrings>
//!     <BusstopStrings>
//!         <String MaxLength="16" Characters="UPPER">IBIS Display</String>
//!     </BusstopStrings>
//! </HOFTemplate>
//! ```
//!
//! OMSI itself ignores the file; openOMSI reads it for the depot file at its side, and the
//! destination the driver types is fitted into the first slots' rules (length, capitals) when
//! one is there. Without a template nothing changes.

use omsi_cfg::CfgFile;
use std::path::{Path, PathBuf};

/// One display string of a template: the slot's name and the rules its text keeps.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Slot {
    /// What the slot is ("BKK-FUTÁR Front display (1. row)").
    pub name: String,
    /// `MaxLength`: the longest text it shows (None when the template gives none).
    pub max_len: Option<usize>,
    /// `Characters="UPPER"`: the display writes in capitals.
    pub upper: bool,
}

impl Slot {
    /// `text` as this slot shows it: capitals when it asks for them, cut to its length.
    pub fn fit(&self, text: &str) -> String {
        let t = text.trim();
        let t = if self.upper {
            t.to_uppercase()
        } else {
            t.to_string()
        };
        match self.max_len {
            Some(n) if t.chars().count() > n => t.chars().take(n).collect(),
            _ => t,
        }
    }
}

/// A parsed `*.tchof.xml`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HofTemplate {
    pub name: String,
    pub description: String,
    pub terminus_slots: Vec<Slot>,
    pub busstop_slots: Vec<Slot>,
}

impl HofTemplate {
    /// Read the template out of the XML text. Lenient on purpose: a pack's template may lack
    /// a section, carry unknown elements or be HTML-escaped in places, and a slot the reader
    /// cannot see is simply not there.
    pub fn parse_text(xml: &str) -> HofTemplate {
        let mut t = HofTemplate {
            name: element_text(xml, "TemplateName").unwrap_or_default(),
            description: element_text(xml, "TemplateDesc").unwrap_or_default(),
            terminus_slots: section_slots(xml, "TerminusStrings"),
            busstop_slots: section_slots(xml, "BusstopStrings"),
        };
        t.name = unescape(&t.name);
        t.description = unescape(&t.description);
        t
    }

    /// The template at `path` (None when it cannot be read).
    pub fn load(path: &Path) -> Option<HofTemplate> {
        let bytes = omsi_cfg::vfs::read(path).ok()?;
        // the stock templates are ISO-8859-1/2; the same decoder the depot files go through
        let f = CfgFile::from_bytes(path, &bytes);
        Some(HofTemplate::parse_text(&f.lines.join("\n")))
    }

    /// Slot `i` of the termini (None when the template has fewer).
    pub fn terminus_slot(&self, i: usize) -> Option<&Slot> {
        self.terminus_slots.get(i)
    }

    /// `text` fitted into terminus slot `i`'s rules; `text` unchanged without such a slot.
    pub fn fit_terminus(&self, i: usize, text: &str) -> String {
        self.terminus_slot(i)
            .map_or_else(|| text.trim().to_string(), |s| s.fit(text))
    }
}

/// The text of the first `<name>…</name>` in `xml`.
fn element_text(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let start = xml.find(&open)?;
    let after = xml[start..].find('>')? + start + 1;
    let close = format!("</{name}>");
    let end = xml[after..].find(&close)? + after;
    Some(xml[after..end].trim().to_string())
}

/// The slots of one `<Section><String …>name</String>…</Section>`.
fn section_slots(xml: &str, section: &str) -> Vec<Slot> {
    let open = format!("<{section}>");
    let close = format!("</{section}>");
    let (Some(a), Some(b)) = (xml.find(&open), xml.find(&close)) else {
        return Vec::new();
    };
    if b < a {
        return Vec::new();
    }
    let body = &xml[a + open.len()..b];
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(i) = rest.find("<String") {
        let tag_start = i;
        let Some(gt) = rest[tag_start..].find('>') else {
            break;
        };
        let attrs = &rest[tag_start + "<String".len()..tag_start + gt];
        let text_start = tag_start + gt + 1;
        let Some(end) = rest[text_start..].find("</String>") else {
            break;
        };
        let text = rest[text_start..text_start + end].trim();
        out.push(Slot {
            name: unescape(text),
            max_len: attr(attrs, "MaxLength")
                .and_then(|v| v.trim().parse::<usize>().ok())
                .filter(|n| *n > 0),
            upper: attr(attrs, "Characters")
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("upper")),
        });
        rest = &rest[text_start + end + "</String>".len()..];
    }
    out
}

/// Attribute `name` of an opening tag's attribute text.
fn attr(attrs: &str, name: &str) -> Option<String> {
    let at = attrs.find(name)?;
    let rest = attrs[at + name.len()..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let body = &rest[1..];
    let end = body.find(quote)?;
    Some(body[..end].to_string())
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The template that belongs to a depot file: `<file stem>.tchof.xml` beside it, else
/// `<[name]>.tchof.xml`, else the only `*.tchof.xml` in the folder (a pack usually ships one
/// template for all of its depot files). Case-insensitive, archives read in place included.
pub fn template_for(hof_path: &Path, hof_name: &str) -> Option<PathBuf> {
    let dir = hof_path.parent()?;
    let mut wanted: Vec<String> = Vec::new();
    if let Some(stem) = hof_path.file_stem() {
        wanted.push(format!("{}.tchof.xml", stem.to_string_lossy()));
    }
    if !hof_name.trim().is_empty() {
        wanted.push(format!("{}.tchof.xml", hof_name.trim()));
    }
    let files: Vec<PathBuf> = omsi_cfg::vfs::read_dir_paths(dir)
        .into_iter()
        .filter(|p| {
            let n = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            n.ends_with(".tchof.xml")
        })
        .collect();
    for w in &wanted {
        let w = w.to_ascii_lowercase();
        if let Some(p) = files.iter().find(|p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase()
                == w
        }) {
            return Some(p.clone());
        }
    }
    (files.len() == 1).then(|| files[0].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of the template attached to #732 (the stock one of the report, shortened).
    const CREATORS: &str = "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>
<HOFTemplate>
    <TemplateName>The Creators HOF</TemplateName>
    <TemplateDesc>This Template can be used for PIS systems
\tOf course, is also compatible with the IBIS system.</TemplateDesc>
    <TerminusStrings>
        <String MaxLength=\"16\" Characters=\"UPPER\">IBIS Display</String>
        <String MaxLength=\"\" Characters=\"NORMAL\">Annax Front (1st row)</String>
        <String MaxLength=\"\" Characters=\"NORMAL\">Annax Front (2nd row)</String>
        <String MaxLength=\"20\" Characters=\"NORMAL\">FOK-GYEM &amp; Vultron</String>
    </TerminusStrings>
    <BusstopStrings>
        <String MaxLength=\"16\" Characters=\"UPPER\">IBIS Display</String>
        <String MaxLength=\"\" Characters=\"NORMAL\">Inner Display (1st row)</String>
    </BusstopStrings>
</HOFTemplate>
";

    #[test]
    fn a_template_names_its_slots_and_their_rules() {
        let t = HofTemplate::parse_text(CREATORS);
        assert_eq!(t.name, "The Creators HOF");
        assert!(t.description.starts_with("This Template can be used"));
        assert_eq!(t.terminus_slots.len(), 4);
        assert_eq!(t.terminus_slots[0].name, "IBIS Display");
        assert_eq!(t.terminus_slots[0].max_len, Some(16));
        assert!(t.terminus_slots[0].upper);
        assert_eq!(t.terminus_slots[1].name, "Annax Front (1st row)");
        assert_eq!(
            t.terminus_slots[1].max_len, None,
            "an empty MaxLength is no limit"
        );
        assert!(!t.terminus_slots[1].upper);
        assert_eq!(
            t.terminus_slots[3].name, "FOK-GYEM & Vultron",
            "the XML escapes are undone"
        );
        assert_eq!(t.busstop_slots.len(), 2);
    }

    #[test]
    fn a_typed_destination_keeps_the_slots_rules() {
        let t = HofTemplate::parse_text(CREATORS);
        // slot 0 wants capitals and 16 characters, slot 1 has no limit
        assert_eq!(t.fit_terminus(0, " központ "), "KÖZPONT");
        assert_eq!(
            t.fit_terminus(0, " köbánya-kispest m "),
            "KÖBÁNYA-KISPEST ",
            "the box shows 16 characters"
        );
        assert_eq!(t.fit_terminus(1, "Kőbánya-Kispest M"), "Kőbánya-Kispest M");
        // a slot the template does not have leaves the text as it is
        assert_eq!(t.fit_terminus(9, "Hauptbahnhof"), "Hauptbahnhof");
        let none = HofTemplate::default();
        assert_eq!(none.fit_terminus(0, " as typed "), "as typed");
    }

    #[test]
    fn a_template_with_no_sections_is_empty_not_a_failure() {
        let t =
            HofTemplate::parse_text("<HOFTemplate><TemplateName>x</TemplateName></HOFTemplate>");
        assert_eq!(t.name, "x");
        assert!(t.terminus_slots.is_empty() && t.busstop_slots.is_empty());
    }

    #[test]
    fn the_template_beside_the_depot_file_is_the_one_used() {
        let dir = std::env::temp_dir().join(format!("omsi-tchof-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let hof = dir.join("Budapest.hof");
        std::fs::write(&hof, b"[name]\nBudapest\n").unwrap();
        // nothing beside it yet
        assert_eq!(template_for(&hof, "Budapest"), None);
        let xml = dir.join("Budapest.TCHOF.XML");
        std::fs::write(&xml, CREATORS.as_bytes()).unwrap();
        assert_eq!(
            template_for(&hof, "Budapest"),
            Some(xml.clone()),
            "the name is read case-insensitively"
        );
        assert_eq!(HofTemplate::load(&xml).unwrap().terminus_slots.len(), 4);
        // a pack with one template for all of its depot files
        std::fs::remove_file(&xml).unwrap();
        let one = dir.join("Shared.tchof.xml");
        std::fs::write(&one, CREATORS.as_bytes()).unwrap();
        assert_eq!(template_for(&hof, "Budapest"), Some(one.clone()));
        // two of them and no name match: nothing is guessed
        std::fs::write(dir.join("Other.tchof.xml"), CREATORS.as_bytes()).unwrap();
        assert_eq!(template_for(&hof, "Budapest"), None);
        // the depot file's own name still wins
        assert_eq!(template_for(&hof, "shared"), Some(one));
        std::fs::remove_dir_all(&dir).ok();
    }
}
