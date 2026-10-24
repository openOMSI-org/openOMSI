//! The variables the passengers' doors are read and written through (`PAX_Entry<n>_Open`,
//! `_Req`, `_Busy`, the same of the exits, and the `door_<k>` / `door<k>` animations),
//! looked up once per vehicle. The people and the timetable asked for them by name every
//! frame, each name put together with `format!` first: some hundred strings a bus a frame.
//! What they answer is the same as by name - the vehicle's variables only ever grow
//! (`set_engine_var`), and then the table is made again.

use super::*;
use omsi_script::VarId;

/// `door_<k>` / `door<k>` kept for k below this; the rest are looked up by name.
const DOOR_ANIMS: usize = 32;

/// See the module. [Entry, Exit] by door number.
#[derive(Debug, Default)]
pub struct PaxVars {
    open: [[Option<VarId>; PAX_DOORS]; 2],
    req: [[Option<VarId>; PAX_DOORS]; 2],
    busy: [[Option<VarId>; PAX_DOORS]; 2],
    /// Whether the scripts report `PAX_<kind><n>_Open`: declare it in a varlist or write it.
    reports_open: [[bool; PAX_DOORS]; 2],
    /// `door_<k>` and `door<k>`.
    door: Vec<(Option<VarId>, Option<VarId>)>,
}

/// "Entry" or "Exit" as the index of [`PaxVars`]'s tables.
fn side(kind: &str) -> Option<usize> {
    match kind {
        "Entry" => Some(0),
        "Exit" => Some(1),
        _ => None,
    }
}

impl PaxVars {
    fn of(v: &VehicleInstance) -> PaxVars {
        let id = |name: &str| omsi_script::compile::with_lower(name, |k| v.var_index.get(k).copied());
        let p = &v.ty.program;
        let mut out = PaxVars::default();
        for (s, kind) in ["Entry", "Exit"].into_iter().enumerate() {
            for i in 0..PAX_DOORS {
                let open = format!("PAX_{kind}{i}_Open");
                out.open[s][i] = id(&open);
                out.req[s][i] = id(&format!("PAX_{kind}{i}_Req"));
                out.busy[s][i] = id(&format!("PAX_{kind}{i}_Busy"));
                out.reports_open[s][i] = reports(p, &open);
            }
        }
        out.door = (0..DOOR_ANIMS).map(|k| (id(&format!("door_{k}")), id(&format!("door{k}")))).collect();
        out
    }
}

/// Whether the program reports variable `name`: declares it in a varlist or writes it
/// (`PeopleSim::script_reports`).
fn reports(p: &Program, name: &str) -> bool {
    p.has_script_var(name) || p.var(name).is_some_and(|id| p.stores(id))
}

impl VehicleInstance {
    fn pax_vars(&self) -> &PaxVars {
        self.pax_vars.get_or_init(|| PaxVars::of(self))
    }

    fn value_of(&self, id: Option<VarId>) -> Option<f32> {
        id.map(|i| self.state.vars[i as usize])
    }

    /// Whether the scripts report `PAX_<kind><i>_Open` (declare it or write it).
    pub fn pax_reports_open(&self, kind: &str, i: usize) -> bool {
        match side(kind) {
            Some(s) if i < PAX_DOORS => self.pax_vars().reports_open[s][i],
            _ => reports(&self.ty.program, &format!("PAX_{kind}{i}_Open")),
        }
    }

    /// `PAX_<kind><i>_Open`, when the vehicle has it.
    pub fn pax_open(&self, kind: &str, i: usize) -> Option<f32> {
        match side(kind) {
            Some(s) if i < PAX_DOORS => self.value_of(self.pax_vars().open[s][i]),
            _ => self.var(&format!("PAX_{kind}{i}_Open")),
        }
    }

    /// Set `PAX_<kind><i>_<what>` (`what` "Req" or "Busy"); false when the vehicle has
    /// no such variable, as [`VehicleInstance::set_var`].
    pub fn set_pax_var(&mut self, kind: &str, i: usize, what: &str, value: f32) -> bool {
        let id = match (side(kind), what) {
            (Some(s), "Req") if i < PAX_DOORS => self.pax_vars().req[s][i],
            (Some(s), "Busy") if i < PAX_DOORS => self.pax_vars().busy[s][i],
            (Some(s), "Open") if i < PAX_DOORS => self.pax_vars().open[s][i],
            _ => return self.set_var(&format!("PAX_{kind}{i}_{what}"), value),
        };
        match id {
            Some(i) => {
                self.state.vars[i as usize] = value;
                true
            }
            None => false,
        }
    }

    /// The door animation `door_<k>`, else `door<k>`, when the vehicle has either.
    pub fn door_anim(&self, k: usize) -> Option<f32> {
        match self.pax_vars().door.get(k) {
            Some(&(a, b)) => self.value_of(a).or_else(|| self.value_of(b)),
            None => self.var(&format!("door_{k}")).or_else(|| self.var(&format!("door{k}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table answers as the names do, for every door, also once the vehicle gained an
    /// engine variable after it was made.
    #[test]
    fn pax_vars_answer_as_by_name() {
        // (some of the built-in variables, a script that writes two of them, a varlist that
        // declares one past the sixteen and door animations of both spellings)
        let dir = std::env::temp_dir().join(format!("omsi_pax_vars_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (script, vars) = (dir.join("doors.osc"), dir.join("vars.txt"));
        std::fs::write(&script, "{frame}\n1 (S.L.PAX_Entry1_Open)\n1 (S.L.PAX_Exit9_Open)\n{end}\n").unwrap();
        std::fs::write(&vars, "door_1\ndoor3\ndoor_40\nPAX_Exit20_Open\n").unwrap();
        let mut builtin: Vec<String> = Vec::new();
        for i in 0..12 {
            for what in ["Open", "Req"] {
                builtin.push(format!("PAX_Entry{i}_{what}"));
                builtin.push(format!("PAX_Exit{i}_{what}"));
            }
        }
        builtin.extend(["PAX_Entry0_Busy", "PAX_Exit3_Busy", "PAX_Entry15_Busy"].map(String::from));
        let program = omsi_script::compile(&CompileInput { builtin_vars: builtin, scripts: vec![script], varlists: vec![vars], ..Default::default() });
        std::fs::remove_dir_all(&dir).ok();
        assert!(program.errors.is_empty(), "{:?}", program.errors);
        let ty = Arc::new(VehicleType {
            def: Default::default(),
            model: Default::default(),
            model_dir: PathBuf::new(),
            program: Arc::new(program),
            meshes: Vec::new(),
            lod_meshes: Vec::new(),
            paint_schemes: Vec::new(),
            texchanges: Vec::new(),
            wheel_meshes: Vec::new(),
            suspension_axles: Vec::new(),
            missing_packs: Vec::new(),
            mesh_bounds: Vec::new(),
            mesh_boxes: Vec::new(),
        });
        let mut v = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
        let by_name = |v: &VehicleInstance| {
            let mut out = Vec::new();
            for kind in ["Entry", "Exit"] {
                for i in 0..PAX_DOORS + 6 {
                    let open = format!("PAX_{kind}{i}_Open");
                    out.push((
                        reports(&v.ty.program, &open),
                        v.var(&open),
                        v.var(&format!("PAX_{kind}{i}_Req")),
                        v.var(&format!("PAX_{kind}{i}_Busy")),
                    ));
                }
            }
            let doors: Vec<Option<f32>> =
                (0..DOOR_ANIMS + 12).map(|k| v.var(&format!("door_{k}")).or_else(|| v.var(&format!("door{k}")))).collect();
            (out, doors)
        };
        let cached = |v: &VehicleInstance| {
            let mut out = Vec::new();
            for kind in ["Entry", "Exit"] {
                for i in 0..PAX_DOORS + 6 {
                    let get = |what: &str| v.var(&format!("PAX_{kind}{i}_{what}"));
                    out.push((v.pax_reports_open(kind, i), v.pax_open(kind, i), get("Req"), get("Busy")));
                }
            }
            let doors: Vec<Option<f32>> = (0..DOOR_ANIMS + 12).map(|k| v.door_anim(k)).collect();
            (out, doors)
        };
        let mut n = 0.0;
        for kind in ["Entry", "Exit"] {
            for i in 0..PAX_DOORS + 6 {
                for what in ["Open", "Req", "Busy"] {
                    n += 1.0;
                    let named = v.var(&format!("PAX_{kind}{i}_{what}")).is_some();
                    assert_eq!(v.set_pax_var(kind, i, what, n), named, "{kind}{i}_{what}");
                }
            }
        }
        for (k, name) in [(1, "door_1"), (3, "door3"), (40, "door_40")] {
            assert!(v.set_var(name, k as f32 / 100.0));
        }
        assert!(v.pax_reports_open("Entry", 1) && v.pax_reports_open("Exit", 9) && v.pax_reports_open("Exit", 20));
        assert!(!v.pax_reports_open("Entry", 2));
        assert_eq!(cached(&v), by_name(&v));
        // a variable the vehicle gains later (none of these, but the table is made again)
        v.set_engine_var("Dirt_Norm", 0.5);
        assert_eq!(cached(&v), by_name(&v));
    }
}
