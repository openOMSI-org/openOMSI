//! Decode every texture file under the given folders and report the ones that fail.
//! usage: decode_all <dir>...
fn main() {
    let mut fails: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    let (mut ok, mut bad) = (0usize, 0usize);
    for root in std::env::args().skip(1) {
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !matches!(ext.as_str(), "bmp" | "tga" | "dds" | "png" | "jpg" | "jpeg") {
                    continue;
                }
                match omsi_texture::decode_file(&p) {
                    Ok(_) => ok += 1,
                    Err(err) => {
                        bad += 1;
                        let key = format!(
                            "{ext}: {}",
                            err.to_string().chars().take(80).collect::<String>()
                        );
                        fails.entry(key).or_default().push(p.display().to_string());
                    }
                }
            }
        }
    }
    println!("{ok} ok, {bad} failed");
    for (k, v) in &fails {
        println!("{:5}  {k}", v.len());
        for f in v.iter().take(3) {
            println!("         {f}");
        }
    }
}
