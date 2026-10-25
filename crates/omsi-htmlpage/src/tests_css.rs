//! CSS the pages use beyond the basics: selectors, values, borders, positions, flexbox.

use super::*;

fn px(frame: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [frame[i], frame[i + 1], frame[i + 2], frame[i + 3]]
}

fn frame(w: u32, h: u32, html: &str) -> Vec<u8> {
    EngineRenderer::new(w, h, html).poll_frame().unwrap()
}

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 128, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const NONE: [u8; 4] = [0, 0, 0, 0];

// ─────────────── selectors ───────────────

#[test]
fn child_and_sibling_combinators() {
    let r = EngineRenderer::new(20, 60, "<style>body{margin:0}p{margin:0;height:2px}#a>p{background:#f00}#a p+p{background:#008000}</style><div id=a><p></p><p></p></div>").poll_frame().unwrap();
    assert_eq!(px(&r, 20, 5, 1), RED, "child combinator");
    assert_eq!(px(&r, 20, 5, 3), GREEN, "adjacent sibling (later rule, same specificity wins by order)");
}

#[test]
fn nth_child_and_not_and_attribute_selectors() {
    let li = "li{display:block;height:2px;margin:0}";
    let r = EngineRenderer::new(20, 20, &format!("<style>body{{margin:0}}ul{{margin:0;padding:0}}{li}li:first-child{{background:#00f}}li:nth-child(even){{background:#f00}}li:last-child{{background:#008000}}</style><ul><li></li><li></li><li></li><li></li></ul>")).poll_frame().unwrap();
    assert_eq!(px(&r, 20, 5, 1), BLUE);
    assert_eq!(px(&r, 20, 5, 3), RED);
    assert_eq!(px(&r, 20, 5, 5), NONE, "the third is neither even, first nor last");
    assert_eq!(px(&r, 20, 5, 7), GREEN, "last-child comes after even in the sheet");
    let r = EngineRenderer::new(20, 20, "<style>body{margin:0}div{height:2px}.x:not(.y){background:#f00}[data-k=v]{background:#00f}</style><div class=x></div><div class='x y'></div><div data-k=v></div>").poll_frame().unwrap();
    assert_eq!(px(&r, 20, 5, 1), RED);
    assert_eq!(px(&r, 20, 5, 3), NONE);
    assert_eq!(px(&r, 20, 5, 5), BLUE);
}

#[test]
fn hover_and_pseudo_elements_never_match() {
    let r = EngineRenderer::new(10, 10, "<style>body{margin:0}div{height:4px}div:hover{background:#f00}div::before{background:#f00}</style><div></div>").poll_frame().unwrap();
    assert_eq!(px(&r, 10, 5, 2), NONE);
}

#[test]
fn query_selector_uses_the_same_selectors() {
    let r = EngineRenderer::new(8, 8, "<body><div id=a><span class=k>x</span><span class=k id=second>y</span></div><p id=o></p><script>document.getElementById('o').textContent = document.querySelector('#a > .k:last-child').id;</script></body>");
    assert_eq!(r.text_of("o").as_deref(), Some("second"));
}

// ─────────────── values ───────────────

#[test]
fn calc_var_and_units() {
    let html = "<style>:root{--w:12px}body{margin:0}#a{width:calc(50% - 4px);height:var(--h, 3px);background:#f00}#b{width:var(--w);height:calc(2 * 2px);background:#00f}#c{width:min(30px,10px);height:max(1px,2px);background:#008000}</style>\
        <div id=a></div><div id=b></div><div id=c></div>";
    let f = frame(40, 20, html);
    // #a: 50% of 40 = 20, less 4 = 16 wide, 3 tall
    assert_eq!(px(&f, 40, 15, 1), RED);
    assert_eq!(px(&f, 40, 16, 1), NONE);
    assert_eq!(px(&f, 40, 17, 1), NONE);
    assert_eq!(px(&f, 40, 17, 4), NONE, "3px tall: below it only #b, which is 12 wide");
    // #b: 12 wide (var), 4 tall
    assert_eq!(px(&f, 40, 11, 4), BLUE);
    assert_eq!(px(&f, 40, 12, 4), NONE);
    // #c: min(30px, 10px) = 10 wide, 2 tall
    assert_eq!(px(&f, 40, 9, 8), GREEN);
    assert_eq!(px(&f, 40, 10, 8), NONE);
}

#[test]
fn custom_properties_inherit_and_important_wins() {
    let html = "<style>body{margin:0;--c:#00f}div{height:4px;background:var(--c)}#a{background:#f00 !important}#a{background:#00f}</style><body><div id=a></div><div></div></body>";
    let f = frame(10, 10, html);
    assert_eq!(px(&f, 10, 2, 1), RED);
    assert_eq!(px(&f, 10, 2, 5), BLUE);
}

#[test]
fn hsl_and_more_colour_names() {
    assert_eq!(parse_color("hsl(120, 100%, 50%)"), Some([0, 255, 0, 255]));
    assert_eq!(parse_color("hsl(0,100%,50%)"), Some(RED));
    assert_eq!(parse_color("navy"), Some([0, 0, 128, 255]));
}

// ─────────────── boxes ───────────────

#[test]
fn borders_take_room_and_are_painted() {
    let html = "<style>body{margin:0}#a{width:10px;height:6px;border:2px solid #f00;background:#00f}</style><div id=a></div>";
    let f = frame(20, 20, html);
    // content-box: 10 + 2*2 = 14 wide, 6 + 4 = 10 tall
    assert_eq!(px(&f, 20, 0, 0), RED, "the corner is border");
    assert_eq!(px(&f, 20, 13, 9), RED, "the far corner is border");
    assert_eq!(px(&f, 20, 14, 5), NONE, "nothing beyond the border box");
    assert_eq!(px(&f, 20, 5, 5), BLUE, "inside is the background");
    assert_eq!(px(&f, 20, 1, 5), RED, "left border is 2 wide");
    assert_eq!(px(&f, 20, 2, 5), BLUE);
}

#[test]
fn border_box_sizing_counts_the_border_in_the_width() {
    let html = "<style>body{margin:0}#a{box-sizing:border-box;width:10px;height:10px;padding:1px;border:1px solid #f00;background:#00f}</style><div id=a></div>";
    let f = frame(20, 20, html);
    assert_eq!(px(&f, 20, 9, 9), RED, "10 wide in all");
    assert_eq!(px(&f, 20, 10, 5), NONE);
    assert_eq!(px(&f, 20, 4, 4), BLUE);
}

#[test]
fn single_sides_and_rounded_borders() {
    let html = "<style>body{margin:0}#a{width:10px;height:10px;border-bottom:3px solid #f00}#b{width:10px;height:10px;border:2px solid #f00;border-radius:5px;margin-top:20px}</style><div id=a></div><div id=b></div>";
    let f = frame(30, 50, html);
    assert_eq!(px(&f, 30, 5, 11), RED);
    assert_eq!(px(&f, 30, 5, 5), NONE);
    // the rounded one: the very corner is cut away, the middle of the top edge is not
    assert_eq!(px(&f, 30, 0, 33)[3], 0, "outside the rounded corner");
    assert_eq!(px(&f, 30, 7, 33), RED, "top edge");
}

#[test]
fn min_and_max_sizes_clamp() {
    let html = "<style>body{margin:0}div{background:#f00}#a{width:100px;max-width:8px;height:2px}#b{width:2px;min-width:6px;height:2px}</style><div id=a></div><div id=b></div>";
    let f = frame(20, 10, html);
    assert_eq!(px(&f, 20, 7, 1), RED);
    assert_eq!(px(&f, 20, 8, 1), NONE);
    assert_eq!(px(&f, 20, 5, 3), RED);
    assert_eq!(px(&f, 20, 6, 3), NONE);
}

#[test]
fn auto_margins_centre_a_box() {
    let html = "<style>body{margin:0}#a{width:10px;height:2px;margin:0 auto;background:#f00}#b{width:10px;height:2px;margin-left:auto;background:#00f}</style><div id=a></div><div id=b></div>";
    let f = frame(30, 10, html);
    assert_eq!(px(&f, 30, 9, 1), NONE);
    assert_eq!(px(&f, 30, 10, 1), RED);
    assert_eq!(px(&f, 30, 19, 1), RED);
    assert_eq!(px(&f, 30, 20, 1), NONE);
    assert_eq!(px(&f, 30, 25, 3), BLUE, "pushed to the right edge");
    assert_eq!(px(&f, 30, 19, 3), NONE);
}

// ─────────────── positioning ───────────────

#[test]
fn absolute_boxes_sit_in_their_positioned_parent() {
    let html = "<style>body{margin:0}#p{position:relative;margin:5px;width:20px;height:20px;background:#00f}#c{position:absolute;right:2px;bottom:2px;width:4px;height:4px;background:#f00}#d{position:absolute;top:0;left:0;width:2px;height:2px;background:#008000}</style><div id=p><div id=c></div><div id=d></div></div>";
    let f = frame(40, 40, html);
    // the parent is at (5,5) 20x20; #c in its bottom right corner, 2 px in
    assert_eq!(px(&f, 40, 5 + 20 - 2 - 1, 5 + 20 - 2 - 1), RED);
    assert_eq!(px(&f, 40, 5 + 20 - 2 - 4 - 1, 5 + 20 - 2 - 1), BLUE);
    assert_eq!(px(&f, 40, 5, 5), GREEN, "#d at the top left");
}

#[test]
fn absolute_without_a_positioned_ancestor_uses_the_page() {
    let html = "<style>body{margin:0}#a{position:absolute;top:10px;left:12px;width:4px;height:4px;background:#f00}</style><div style='height:3px'></div><div id=a></div>";
    let f = frame(30, 30, html);
    assert_eq!(px(&f, 30, 13, 11), RED);
    assert_eq!(px(&f, 30, 11, 11), NONE);
    assert_eq!(px(&f, 30, 13, 9), NONE);
}

#[test]
fn relative_position_shifts_without_moving_the_flow() {
    let html = "<style>body{margin:0}div{height:4px;width:4px}#a{position:relative;left:6px;top:1px;background:#f00}#b{background:#00f}</style><div id=a></div><div id=b></div>";
    let f = frame(20, 20, html);
    assert_eq!(px(&f, 20, 7, 2), RED);
    assert_eq!(px(&f, 20, 2, 5), BLUE, "the next box is where it would have been");
}

#[test]
fn z_index_orders_absolute_boxes() {
    let html = "<style>body{margin:0}.a{position:absolute;top:0;left:0;width:6px;height:6px}#lo{background:#f00;z-index:1}#hi{background:#00f;z-index:2}</style><div id=hi class=a></div><div id=lo class=a></div>";
    let f = frame(10, 10, html);
    assert_eq!(px(&f, 10, 2, 2), BLUE, "the higher z-index is on top whatever the order in the file");
}

#[test]
fn overflow_hidden_clips_children() {
    let html = "<style>body{margin:0}#a{width:6px;height:6px;overflow:hidden}#b{margin-left:3px;width:20px;height:4px;background:#f00}</style><div id=a><div id=b></div></div>";
    let f = frame(30, 10, html);
    assert_eq!(px(&f, 30, 4, 1), RED);
    assert_eq!(px(&f, 30, 7, 1), NONE, "beyond the clip box");
}

#[test]
fn opacity_fades_the_whole_subtree() {
    let html = "<style>body{margin:0}#a{opacity:0.5;width:4px;height:4px;background:#f00}</style><div id=a></div>";
    let f = frame(10, 10, html);
    let p = px(&f, 10, 1, 1);
    assert_eq!((p[0], p[1], p[2]), (255, 0, 0));
    assert!((p[3] as i32 - 128).abs() <= 2, "{p:?}");
}

#[test]
fn linear_gradients_and_shadows() {
    let html = "<style>body{margin:0}#a{width:10px;height:4px;background:linear-gradient(to right, #f00, #00f)}#b{margin:20px 10px;width:6px;height:6px;background:#fff;box-shadow:0 0 6px #000}</style><div id=a></div><div id=b></div>";
    let f = frame(40, 50, html);
    let l = px(&f, 40, 0, 1);
    let r = px(&f, 40, 9, 1);
    assert!(l[0] > 200 && l[2] < 55, "{l:?}");
    assert!(r[2] > 200 && r[0] < 55, "{r:?}");
    // the shadow: a faint dark halo outside the white box, none far from it
    let halo = px(&f, 40, 8, 26 + 3);
    assert!(halo[3] > 0 && halo[0] < 50, "{halo:?}");
    assert_eq!(px(&f, 40, 30, 30), NONE);
}

// ─────────────── text ───────────────

#[test]
fn text_transform_and_nowrap_ellipsis() {
    let r = EngineRenderer::new(8, 8, "<body><p id=o style='text-transform:uppercase'>abc</p><script>var e = document.getElementById('o');</script></body>");
    // (the transform is for painting: the DOM text stays as written)
    assert_eq!(r.text_of("o").as_deref(), Some("abc"));
    // nowrap keeps one line even where it overflows: the second word is not pushed down
    let html = "<style>body{margin:0;font-size:10px;line-height:1}#a{width:10px;white-space:nowrap;color:#f00}</style><body><div id=a>MMMM MMMM</div></body>";
    let f = frame(60, 30, html);
    let below = (10..30).any(|y| (0..60).any(|x| px(&f, 60, x, y)[3] > 0));
    assert!(!below, "no second line");
}

// ─────────────── flexbox ───────────────

fn flex_page(container: &str, kids: &str) -> String {
    format!("<style>body{{margin:0}}#f{{{container}}}#f>div{{height:4px}}.r{{background:#f00}}.g{{background:#008000}}.b{{background:#00f}}</style><body><div id=f>{kids}</div></body>")
}

#[test]
fn a_row_puts_items_side_by_side() {
    let f = frame(40, 10, &flex_page("display:flex;width:40px", "<div class=r style='width:10px'></div><div class=g style='width:10px'></div>"));
    assert_eq!(px(&f, 40, 5, 1), RED);
    assert_eq!(px(&f, 40, 15, 1), GREEN);
    assert_eq!(px(&f, 40, 25, 1), NONE);
}

#[test]
fn grow_shares_the_free_space() {
    let f = frame(40, 10, &flex_page("display:flex;width:40px", "<div class=r style='flex:1'></div><div class=g style='flex:3'></div>"));
    // 10 and 30
    assert_eq!(px(&f, 40, 9, 1), RED);
    assert_eq!(px(&f, 40, 10, 1), GREEN);
    assert_eq!(px(&f, 40, 39, 1), GREEN);
}

#[test]
fn shrink_squeezes_items_that_do_not_fit() {
    let f = frame(40, 10, &flex_page("display:flex;width:30px", "<div class=r style='width:20px'></div><div class=g style='width:20px'></div>"));
    // 40 wanted in 30: 15 each
    assert_eq!(px(&f, 40, 14, 1), RED);
    assert_eq!(px(&f, 40, 15, 1), GREEN);
    assert_eq!(px(&f, 40, 29, 1), GREEN);
    assert_eq!(px(&f, 40, 30, 1), NONE);
}

#[test]
fn justify_content_places_the_free_space() {
    let kids = "<div class=r style='width:10px'></div><div class=g style='width:10px'></div>";
    let centre = frame(40, 10, &flex_page("display:flex;width:40px;justify-content:center", kids));
    assert_eq!(px(&centre, 40, 9, 1), NONE);
    assert_eq!(px(&centre, 40, 10, 1), RED);
    assert_eq!(px(&centre, 40, 29, 1), GREEN);
    let end = frame(40, 10, &flex_page("display:flex;width:40px;justify-content:flex-end", kids));
    assert_eq!(px(&end, 40, 19, 1), NONE);
    assert_eq!(px(&end, 40, 20, 1), RED);
    let between = frame(40, 10, &flex_page("display:flex;width:40px;justify-content:space-between", kids));
    assert_eq!(px(&between, 40, 0, 1), RED);
    assert_eq!(px(&between, 40, 39, 1), GREEN);
    assert_eq!(px(&between, 40, 20, 1), NONE);
}

#[test]
fn gap_separates_items() {
    let f = frame(40, 10, &flex_page("display:flex;gap:5px", "<div class=r style='width:10px'></div><div class=g style='width:10px'></div>"));
    assert_eq!(px(&f, 40, 9, 1), RED);
    assert_eq!(px(&f, 40, 12, 1), NONE);
    assert_eq!(px(&f, 40, 15, 1), GREEN);
}

#[test]
fn align_items_in_the_cross_axis() {
    let kids = "<div class=r style='width:6px;height:2px'></div>";
    let page = |align: &str| format!("<style>body{{margin:0}}#f{{display:flex;height:10px;align-items:{align}}}.r{{background:#f00}}</style><body><div id=f>{kids}</div></body>");
    let start = frame(20, 12, &page("flex-start"));
    assert_eq!(px(&start, 20, 2, 1), RED);
    assert_eq!(px(&start, 20, 2, 5), NONE);
    let centre = frame(20, 12, &page("center"));
    assert_eq!(px(&centre, 20, 2, 1), NONE);
    assert_eq!(px(&centre, 20, 2, 5), RED);
    let end = frame(20, 12, &page("flex-end"));
    assert_eq!(px(&end, 20, 2, 9), RED);
    // stretch (the default) makes an auto-height item as tall as the line
    let stretch = frame(20, 12, "<style>body{margin:0}#f{display:flex;height:10px}.r{background:#f00;width:6px}</style><body><div id=f><div class=r></div></div></body>");
    assert_eq!(px(&stretch, 20, 2, 9), RED);
}

#[test]
fn a_column_stacks_and_stretches_across() {
    let f = frame(20, 30, "<style>body{margin:0}#f{display:flex;flex-direction:column;width:20px}.r{background:#f00;height:4px}.g{background:#008000;height:4px}</style><body><div id=f><div class=r></div><div class=g></div></div></body>");
    assert_eq!(px(&f, 20, 19, 1), RED, "stretched to the full width");
    assert_eq!(px(&f, 20, 19, 5), GREEN);
    assert_eq!(px(&f, 20, 2, 9), NONE);
}

#[test]
fn a_column_with_a_height_shares_it_by_grow() {
    let f = frame(20, 40, "<style>body{margin:0}#f{display:flex;flex-direction:column;height:30px}.r{background:#f00;flex:1}.g{background:#008000;flex:2}</style><body><div id=f><div class=r></div><div class=g></div></div></body>");
    assert_eq!(px(&f, 20, 2, 9), RED);
    assert_eq!(px(&f, 20, 2, 10), GREEN);
    assert_eq!(px(&f, 20, 2, 29), GREEN);
    assert_eq!(px(&f, 20, 2, 30), NONE);
}

#[test]
fn wrap_starts_a_new_line() {
    let f = frame(30, 20, &flex_page("display:flex;flex-wrap:wrap;width:25px", "<div class=r style='width:10px'></div><div class=g style='width:10px'></div><div class=b style='width:10px'></div>"));
    assert_eq!(px(&f, 30, 5, 1), RED);
    assert_eq!(px(&f, 30, 15, 1), GREEN);
    assert_eq!(px(&f, 30, 25, 1), NONE, "the third does not fit on the line");
    assert_eq!(px(&f, 30, 5, 5), BLUE, "it starts the next one");
}

#[test]
fn auto_margins_push_flex_items_apart() {
    let f = frame(40, 10, &flex_page("display:flex;width:40px", "<div class=r style='width:10px'></div><div class=g style='width:10px;margin-left:auto'></div>"));
    assert_eq!(px(&f, 40, 5, 1), RED);
    assert_eq!(px(&f, 40, 20, 1), NONE);
    assert_eq!(px(&f, 40, 35, 1), GREEN);
}

#[test]
fn centring_with_flex() {
    let f = frame(20, 20, "<style>body{margin:0}#f{display:flex;justify-content:center;align-items:center;width:20px;height:20px}#c{width:6px;height:6px;background:#f00}</style><body><div id=f><div id=c></div></div></body>");
    assert_eq!(px(&f, 20, 10, 10), RED);
    assert_eq!(px(&f, 20, 6, 10), NONE);
    assert_eq!(px(&f, 20, 10, 6), NONE);
}

#[test]
fn order_and_reverse() {
    let f = frame(40, 10, &flex_page("display:flex", "<div class=r style='width:10px;order:2'></div><div class=g style='width:10px;order:1'></div>"));
    assert_eq!(px(&f, 40, 5, 1), GREEN);
    assert_eq!(px(&f, 40, 15, 1), RED);
    let r = frame(40, 10, &flex_page("display:flex;flex-direction:row-reverse;width:40px", "<div class=r style='width:10px'></div><div class=g style='width:10px'></div>"));
    assert_eq!(px(&r, 40, 35, 1), RED, "row-reverse starts at the right edge");
    assert_eq!(px(&r, 40, 25, 1), GREEN);
}

#[test]
fn text_in_flex_items_and_nested_flex() {
    let html = "<style>body{margin:0;font-size:10px}#f{display:flex;width:60px}#f>div{flex:1}.r{background:#f00}.g{background:#008000}#in{display:flex;flex-direction:column}</style><body><div id=f><div class=r>AB</div><div class=g id=in><div>x</div><div>y</div></div></div></body>";
    let f = frame(60, 40, html);
    assert_eq!(px(&f, 60, 29, 15), RED, "both columns are as tall as the tallest: 2 lines");
    assert_eq!(px(&f, 60, 59, 15), GREEN);
}
