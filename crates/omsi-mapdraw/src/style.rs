//! How the maps look: one palette for the light look (after Apple Maps: green land, white
//! roads with a fine grey edge, pale houses lit from one side with soft shadows, a blue
//! route) and one for the dark look, shared by the small map, the city map and the launcher's
//! picture.

use omsi_ui::Color;

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// The land outside the map's tiles and its plain ground.
    pub land: Color,
    pub grass: Color,
    pub forest: Color,
    pub field: Color,
    pub paved_land: Color,
    pub sand: Color,
    pub water: Color,
    /// Carriageways: the surface and its edge; pavements; yards and car parks.
    pub road: Color,
    pub road_edge: Color,
    pub foot: Color,
    pub area: Color,
    /// Houses: the roof, the wall facing the light and the one turned from it, the shadow.
    pub roof: Color,
    pub roof_edge: Color,
    pub wall_lit: Color,
    pub wall_dark: Color,
    pub shadow: Color,
    pub tree: Color,
    pub tree_edge: Color,
    /// Railways (Google's way: a grey band with white dashes) and tram tracks.
    pub rail: Color,
    pub rail_dash: Color,
    pub tram: Color,
    /// The route and its edge, the part already driven.
    pub route: Color,
    pub route_edge: Color,
    pub driven: Color,
    /// The cards over the map and their texts.
    pub card: Color,
    pub card_text: Color,
    pub card_dim: Color,
    pub label: Color,
    pub label_halo: Color,
    /// The ground's contour lines (the terrain view).
    pub contour: Color,
}

const fn hex(v: u32) -> Color {
    Color::hex(v)
}

pub const LIGHT: Palette = Palette {
    land: hex(0xdcebcb),
    grass: hex(0xdcebcb),
    forest: hex(0xbcdba5),
    field: hex(0xebebcf),
    paved_land: hex(0xedebe6),
    sand: hex(0xf1e8cf),
    water: hex(0x9fcff2),
    road: hex(0xffffff),
    road_edge: hex(0xcfccc4),
    foot: hex(0xe6e3dc),
    area: hex(0xefeef0),
    roof: hex(0xfaf9f6),
    roof_edge: hex(0xdcd8d0),
    wall_lit: hex(0xe9e5de),
    wall_dark: hex(0xc3bcb0),
    shadow: Color::rgba(40, 58, 46, 0.15),
    tree: hex(0xa9d38f),
    tree_edge: hex(0x92c27a),
    rail: hex(0xa7a59f),
    rail_dash: hex(0xffffff),
    tram: hex(0x9a95a6),
    route: hex(0x2f7cf6),
    route_edge: hex(0x1a5fcf),
    driven: hex(0x9fb4d3),
    card: Color::rgba(255, 255, 255, 0.94),
    card_text: hex(0x1c1c1e),
    card_dim: hex(0x6e6e73),
    label: hex(0x4a4a4f),
    label_halo: Color::rgba(255, 255, 255, 0.9),
    contour: Color::rgba(150, 105, 60, 1.0),
};

pub const DARK: Palette = Palette {
    land: hex(0x27332b),
    grass: hex(0x27332b),
    forest: hex(0x203528),
    field: hex(0x2f3229),
    paved_land: hex(0x2c2e30),
    sand: hex(0x34322a),
    water: hex(0x1d3c55),
    road: hex(0x62666e),
    road_edge: hex(0x2b2e33),
    foot: hex(0x45484e),
    area: hex(0x393b40),
    roof: hex(0x3b3f47),
    roof_edge: hex(0x2c2f35),
    wall_lit: hex(0x363a41),
    wall_dark: hex(0x26292e),
    shadow: Color::rgba(0, 0, 0, 0.22),
    tree: hex(0x2f4a35),
    tree_edge: hex(0x26402c),
    rail: hex(0x6d6f75),
    rail_dash: hex(0x2a2c2f),
    tram: hex(0x6a6676),
    route: hex(0x3d8bff),
    route_edge: hex(0x1d5bc0),
    driven: hex(0x4a5a74),
    card: Color::rgba(30, 30, 32, 0.94),
    card_text: hex(0xf2f2f7),
    card_dim: hex(0xa1a1a8),
    label: hex(0xd6d6db),
    label_halo: Color::rgba(20, 20, 22, 0.85),
    contour: Color::rgba(190, 150, 100, 1.0),
};

pub fn palette(light: bool) -> &'static Palette {
    if light {
        &LIGHT
    } else {
        &DARK
    }
}

/// Traffic on a road: free (the route's own blue), slow, heavy, standing.
pub const TRAFFIC: [Color; 4] = [hex(0x2f7cf6), hex(0xff9f0a), hex(0xff453a), hex(0xa4161a)];

/// Which way the shadows fall, per metre of height (world, x east, y north): the light comes
/// from the north-west, as on a printed map lit from its top left corner.
pub const SHADOW_DIR: [f32; 2] = [0.34, -0.28];
