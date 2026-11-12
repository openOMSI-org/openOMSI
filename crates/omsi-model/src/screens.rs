//! `[htmltexture]` pages and `[navigatortexture]` screens: script texture slots that are not
//! the scripts' (see `docs/HTMLTEXTURE.md`, `docs/NAVIGATORTEXTURE.md`).

use super::*;

pub const NAVIGATOR_TEXTURE_MAX: i32 = 4096;

/// An openOMSI extension: Omsi.exe leaves such a slot an empty script texture.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NavigatorTextureDef {
    pub script_index: usize,
    pub width: i32,
    pub height: i32,
    /// A variable the scripts do not have leaves the screen on, as no variable does.
    pub power_var: Option<String>,
}

impl NavigatorTextureDef {
    pub fn is_on(&self, var: impl Fn(&str) -> Option<f32>) -> bool {
        self.power_var.as_deref().and_then(var).is_none_or(|v| v != 0.0)
    }
}

impl Model {
    pub(super) fn reserve_script_texture(&mut self, width: i32, height: i32) -> usize {
        self.script_textures.push((width, height));
        self.script_textures.len() - 1
    }

    pub(super) fn bind_script_texture(&mut self, script_index: Option<usize>) {
        if let (Some(m), Some(i)) = (self.cur_matl(), script_index) {
            m.use_script_texture = Some(i as i32);
        }
    }

    /// The size is held to what every GPU can make a render target of.
    pub(super) fn parse_navigator_texture(&mut self, r: &mut CfgReader<'_>) {
        let width = r.i32().clamp(1, NAVIGATOR_TEXTURE_MAX);
        let height = r.i32().clamp(1, NAVIGATOR_TEXTURE_MAX);
        let power_var = Self::take_optional_word(r);
        let script_index = self.reserve_script_texture(width, height);
        self.navigator_textures.push(NavigatorTextureDef { script_index, width, height, power_var });
    }

    pub(super) fn parse_use_navigator_texture(&mut self, r: &mut CfgReader<'_>) {
        let index = usize::try_from(r.i32()).ok().and_then(|n| self.navigator_textures.get(n)).map(|d| d.script_index);
        self.bind_script_texture(index);
    }

    fn take_optional_word(r: &mut CfgReader<'_>) -> Option<String> {
        let mut ahead = r.clone();
        let word = ahead.param_line().trim();
        (!word.is_empty()).then(|| {
            *r = ahead;
            word.to_string()
        })
    }

    pub fn navigator_screen(&self, script_index: usize) -> Option<&NavigatorTextureDef> {
        self.navigator_textures.iter().find(|n| n.script_index == script_index)
    }

    /// Neither a script nor an upload of pixels may fill such a slot.
    pub fn script_texture_drawn_by_game(&self, script_index: usize) -> bool {
        self.navigator_screen(script_index).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Model {
        Model::parse(&CfgFile::from_str("model.cfg", text))
    }

    #[test]
    fn a_navigator_screen_takes_a_script_texture_index() {
        let m = parse("[scripttexture]\n64\n32\n\n[navigatortexture]\n800\n480\n\n[mesh]\nx.o3d\n\n[matl]\ngps.dds\n0\n[useNavigatorTexture]\n0\n");
        assert_eq!(m.script_textures, vec![(64, 32), (800, 480)]);
        assert_eq!(m.navigator_textures, vec![NavigatorTextureDef { script_index: 1, width: 800, height: 480, power_var: None }]);
        assert_eq!(m.meshes[0].materials[0].use_script_texture, Some(1));
        assert!(m.script_texture_drawn_by_game(1) && !m.script_texture_drawn_by_game(0));
        assert!(m.navigator_screen(0).is_none());
    }

    #[test]
    fn the_power_variable_is_optional_and_the_next_keyword_is_still_read() {
        let m = parse("[navigatortexture]\n400\n240\nelec_busbar_av\n[mesh]\nx.o3d\n[matl]\ngps.dds\n0\n[useNavigatorTexture]\n0\n");
        assert_eq!(m.navigator_textures[0].power_var.as_deref(), Some("elec_busbar_av"));
        assert_eq!(m.meshes.len(), 1, "the [mesh] after it is a keyword, not the variable");
        assert_eq!(m.meshes[0].materials[0].use_script_texture, Some(0));
        let m = parse("[navigatortexture]\n400\n240\n[mesh]\nx.o3d\n");
        assert_eq!(m.navigator_textures[0].power_var, None);
        assert_eq!(m.meshes.len(), 1);
    }

    #[test]
    fn a_screens_size_is_held_to_what_a_gpu_can_make() {
        let m = parse("[navigatortexture]\n99999\n0\n");
        assert_eq!((m.navigator_textures[0].width, m.navigator_textures[0].height), (NAVIGATOR_TEXTURE_MAX, 1));
        assert_eq!(m.script_textures, vec![(NAVIGATOR_TEXTURE_MAX, 1)], "the slot is made at that size too");
    }

    #[test]
    fn pages_and_screens_number_their_slots_together() {
        let m = parse("[htmltexture]\n64\n32\nsign.html\n\n[navigatortexture]\n800\n480\n\n[htmltexture]\n64\n32\nother.html\n\n[mesh]\nx.o3d\n[matl]\na.dds\n0\n[useHtmlTexture]\n1\n[matl]\nb.dds\n0\n[useNavigatorTexture]\n0\n[matl]\nc.dds\n0\n[useNavigatorTexture]\n5\n");
        assert_eq!(m.script_textures.len(), 3);
        assert_eq!((m.html_textures[1].script_index, m.navigator_textures[0].script_index), (2, 1));
        let used: Vec<_> = m.meshes[0].materials.iter().map(|x| x.use_script_texture).collect();
        assert_eq!(used, vec![Some(2), Some(1), None], "a screen that is not declared binds nothing");
    }

    #[test]
    fn a_screen_is_on_while_its_variable_is_not_zero() {
        let always = NavigatorTextureDef::default();
        assert!(always.is_on(|_| Some(0.0)), "no variable: always on");
        let powered = NavigatorTextureDef { power_var: Some("power".into()), ..Default::default() };
        assert!(!powered.is_on(|_| Some(0.0)));
        assert!(powered.is_on(|n| (n == "power").then_some(1.0)));
        assert!(powered.is_on(|_| None), "a variable the scripts lack leaves it on");
    }
}
