fn main() {
    let args: Vec<String> = std::env::args().collect();
    let img = omsi_texture::decode_file(std::path::Path::new(&args[1])).unwrap();
    println!("{}x{} alpha={}", img.width, img.height, img.has_alpha);
    image::save_buffer(
        &args[2],
        &img.rgba,
        img.width,
        img.height,
        image::ColorType::Rgba8,
    )
    .unwrap();
}
