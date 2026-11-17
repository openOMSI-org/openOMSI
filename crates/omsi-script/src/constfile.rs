//! Script constfiles: `[const] name value` and `[newcurve] name` + `[pnt] x y`.

use omsi_cfg::CfgFile;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Curve {
    pub name: String,
    pub points: Vec<(f32, f32)>,
}

impl Curve {
    /// Piece-wise linear interpolation, clamped to the first/last value outside the range.
    pub fn eval(&self, x: f32) -> f32 {
        let p = &self.points;
        if p.is_empty() {
            return 0.0;
        }
        if x <= p[0].0 {
            return p[0].1;
        }
        let last = p[p.len() - 1];
        if x >= last.0 {
            return last.1;
        }
        for w in p.windows(2) {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            if x >= x0 && x <= x1 {
                if x1 == x0 {
                    return y1;
                }
                return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
            }
        }
        last.1
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConstFile {
    pub consts: Vec<(String, f32)>,
    pub curves: Vec<Curve>,
    /// Non-fatal problems (`SC_pnt_before_newcurve`, …).
    pub errors: Vec<String>,
}

impl ConstFile {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let file = CfgFile::read(path).map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(Self::parse(&file))
    }

    pub fn parse(file: &CfgFile) -> Self {
        let mut out = ConstFile::default();
        let mut r = file.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "const" => {
                    let name = r.word().to_string();
                    let value = r.f32();
                    // Keep source order; the compiler resolves duplicates across the full script set.
                    out.consts.push((name, value));
                }
                "newcurve" => {
                    let name = r.word().to_string();
                    out.curves.push(Curve { name, points: Vec::new() });
                }
                "pnt" => {
                    let x = r.f32();
                    let y = r.f32();
                    match out.curves.last_mut() {
                        Some(c) => c.points.push((x, y)),
                        None => out.errors.push(format!("{}:{}: [pnt] before [newcurve]", file.path.display(), r.block_line())),
                    }
                }
                _ => {}
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::ConstFile;

    #[test]
    fn duplicate_consts_preserve_source_order() {
        let file = omsi_cfg::CfgFile::from_str(
            "duplicate.txt",
            "[const]\nDoorAftOpenTime\n0.8\n[const]\ndooraftopentime\n0\n",
        );
        let parsed = ConstFile::parse(&file);
        assert_eq!(parsed.consts, vec![("DoorAftOpenTime".to_string(), 0.8), ("dooraftopentime".to_string(), 0.0)]);
    }
}
