use super::freetex::*;
use omsi_texture::{Image, TextureData};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

fn image(width: u32, color: u8) -> Arc<TextureData> {
    Arc::new(TextureData::from_image(Image {
        width,
        height: 2,
        rgba: vec![color; width as usize * 8],
        has_alpha: true,
    }))
}

struct Pair {
    a: Worker,
    b: Worker,
    sa: State,
    sb: State,
    a_packets: Vec<Vec<u8>>,
    b_packets: Vec<Vec<u8>>,
    received: Vec<Received>,
    bytes: usize,
    lossy: bool,
    packet: usize,
}

impl Pair {
    fn new() -> Self {
        Self {
            a: Worker::new().unwrap(),
            b: Worker::new().unwrap(),
            sa: State {
                id: 1,
                local: Frame {
                    table: 42,
                    images: vec![Some(image(8, 7))],
                    ..Frame::default()
                },
                peers: vec![2],
                subscriptions: BTreeMap::from([(2, (42, 10))]),
                candidates: Vec::new(),
            },
            sb: State {
                id: 2,
                local: Frame {
                    table: 42,
                    images: vec![Some(image(8, 99))],
                    ..Frame::default()
                },
                peers: vec![1],
                subscriptions: BTreeMap::from([(1, (42, 20))]),
                candidates: Vec::new(),
            },
            a_packets: Vec::new(),
            b_packets: Vec::new(),
            received: Vec::new(),
            bytes: 0,
            lossy: false,
            packet: 0,
        }
    }
    fn step(&mut self) {
        let a = self
            .a
            .pump(self.sa.clone(), std::mem::take(&mut self.a_packets));
        let b = self
            .b
            .pump(self.sb.clone(), std::mem::take(&mut self.b_packets));
        for p in a.packets.iter().chain(&b.packets) {
            assert!(p.len() <= 1245);
            assert_eq!(p[0], omsi_net::freetex::MAGIC);
            if p[2] == 1 {
                self.bytes += p.len();
            }
        }
        let mut pa = a.packets;
        let mut pb = b.packets;
        if self.lossy {
            pa.reverse();
            pb.reverse();
            for packets in [&mut pa, &mut pb] {
                packets.retain(|_| {
                    self.packet += 1;
                    self.packet % 5 != 0
                });
            }
        }
        self.b_packets.extend(pa);
        self.a_packets.extend(pb);
        self.received.extend(a.images);
        self.received.extend(b.images);
        std::thread::sleep(Duration::from_millis(5));
    }
    fn wait(&mut self, test: impl Fn(&[Received]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !test(&self.received) && Instant::now() < deadline {
            self.step();
        }
        assert!(test(&self.received), "pixel transfer timed out: {} bytes, {} committed images", self.bytes, self.received.len());
    }
}

#[test]
fn fragmented_lossy_images_commit_atomically_and_single_pixel_deltas_are_small() {
    let mut pair = Pair::new();
    pair.lossy = true;
    let mut seed = 91u32;
    let rgba: Vec<u8> = (0..128 * 128 * 4)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        })
        .collect();
    let texture = Arc::new(TextureData::from_image(Image {
        width: 128,
        height: 128,
        rgba: rgba.clone(),
        has_alpha: true,
    }));
    pair.sa.local.images[0] = Some(texture.clone());
    pair.wait(|v| v.iter().any(|r| r.incoming.owner == 1));
    let received = pair
        .received
        .iter()
        .find(|r| r.incoming.owner == 1)
        .unwrap();
    assert_eq!(*received.incoming.image.rgba, rgba);
    for _ in 0..80 {
        pair.step();
    }
    pair.received.clear();
    let initial = pair.bytes;
    let mut next = texture.as_ref().clone();
    next.levels[0][123] ^= 127;
    pair.sa.local.images[0] = Some(Arc::new(next));
    pair.wait(|v| {
        v.iter()
            .any(|r| r.incoming.owner == 1 && r.incoming.image.rgba[123] == (rgba[123] ^ 127))
    });
    let delta = pair.bytes - initial;
    assert!(
        delta < initial / 4,
        "delta {delta}, initialization {initial}"
    );
    eprintln!(
        "freetex lossy transfer: initialization {initial} bytes, one-pixel delta {delta} bytes"
    );
}

#[test]
fn pixels_exchange_without_files_and_update_clear_resize_and_reinitialize() {
    let mut pair = Pair::new();
    pair.wait(|v| v.len() == 2);
    assert!(
        pair.received
            .iter()
            .any(|v| v.incoming.owner == 1 && v.incoming.image.rgba[0] == 7)
    );
    assert!(
        pair.received
            .iter()
            .any(|v| v.incoming.owner == 2 && v.incoming.image.rgba[0] == 99)
    );
    pair.received.clear();
    // Settle commit acknowledgements. No unchanged image may be sent again.
    for _ in 0..30 {
        pair.step();
    }
    let bytes = pair.bytes;
    for _ in 0..30 {
        pair.step();
    }
    assert_eq!(pair.bytes, bytes);
    assert!(pair.received.is_empty());
    // Same slot, new pixels: no path or filename needs to change.
    pair.sa.local.images[0] = Some(image(8, 13));
    pair.wait(|v| {
        v.iter()
            .any(|r| r.incoming.owner == 1 && r.incoming.image.rgba[0] == 13)
    });
    pair.received.clear();
    pair.sa.local.images[0] = None;
    pair.wait(|v| v.iter().any(|r| r.incoming.owner == 1 && r.data.is_none()));
    pair.received.clear();
    pair.sa.local.images[0] = Some(image(17, 21));
    pair.wait(|v| {
        v.iter().any(|r| {
            r.incoming.owner == 1
                && r.data
                    .as_ref()
                    .is_some_and(|d| d.width == 17 && d.levels[0][0] == 21)
        })
    });
    pair.received.clear();
    pair.sb.subscriptions.insert(1, (42, 30));
    pair.wait(|v| {
        v.iter()
            .any(|r| r.incoming.owner == 1 && r.generation == 30 && r.incoming.image.width == 17)
    });
}

#[test]
fn differing_layout_never_receives_pixels_and_frame_submission_does_not_wait() {
    let mut pair = Pair::new();
    pair.sb.subscriptions.insert(1, (999, 20));
    for _ in 0..60 {
        pair.step();
    }
    assert!(pair.received.iter().all(|r| r.incoming.owner != 1));
    let start = Instant::now();
    for _ in 0..100 {
        let _ = pair.a.pump(pair.sa.clone(), Vec::new());
    }
    assert!(
        start.elapsed() < Duration::from_millis(250),
        "frame waited on the image worker"
    );
}

#[test]
fn matching_file_and_pixels_sha_skip_pixels_but_same_name_different_file_does_not() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("omsi-freetex-sha-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(root.join("a")).unwrap();
    std::fs::create_dir_all(root.join("b")).unwrap();
    let a = root.join("a/display.png");
    let b = root.join("b/display.png");
    std::fs::write(&a, b"identical file bytes").unwrap();
    std::fs::write(&b, b"identical file bytes").unwrap();
    let mut seed = 71u32;
    let rgba = (0..256 * 256 * 4)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        })
        .collect::<Vec<_>>();
    let data = || {
        Arc::new(TextureData::from_image(Image {
            width: 256,
            height: 256,
            rgba: rgba.clone(),
            has_alpha: true,
        }))
    };
    let mut pair = Pair::new();
    pair.lossy = true;
    pair.sa.local.images = vec![Some(data())];
    pair.sb.local.images = vec![Some(data())];
    pair.sa.local.paths = vec![Some(a.clone())];
    pair.sb.local.paths = vec![Some(b.clone())];
    pair.wait(|v| v.len() == 2);
    assert!(pair.received.iter().all(|r| *r.incoming.image.rgba == rgba));
    assert!(
        pair.bytes < 2000,
        "SHA matches must exchange references only, got {} bytes",
        pair.bytes
    );
    eprintln!(
        "matching SHA bidirectional 256x256 image: {} data/reference bytes",
        pair.bytes
    );
    // Same filenames and identical rendered pixels, but differing file bytes: the file
    // SHA still must fail, not silently reuse the same-named picture.
    std::fs::write(&b, b"different file bytes").unwrap();
    pair.sb.local.images[0] = Some(data());
    pair.sb.subscriptions.insert(1, (42, 21));
    pair.sa.subscriptions.insert(2, (42, 11));
    pair.received.clear();
    let before = pair.bytes;
    pair.wait(|v| {
        v.iter().any(|r| r.incoming.owner == 1 && r.generation == 21)
            && v.iter().any(|r| r.incoming.owner == 2 && r.generation == 11)
    });
    assert!(
        pair.bytes - before >= rgba.len(),
        "different file SHA must transfer pixels"
    );
    eprintln!(
        "different SHA, same names: {} data/reference bytes",
        pair.bytes - before
    );
    std::fs::remove_dir_all(root).unwrap();
}
