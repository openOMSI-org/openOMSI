#[test]
fn the_arrows_come_in_a_pair() {
    for name in ["arrow_back", "arrow_forward"] {
        let pixels = omsi_ui::icons::rasterize(name, 32).expect(name);
        assert!(pixels.iter().any(|alpha| *alpha > 0), "{name}");
    }
}
