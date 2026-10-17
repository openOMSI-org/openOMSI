use super::super::Manifest;
use super::*;
use crate::mods::budget::{kept_lists, transfers, TRANSFER_MEMORY};
use std::net::TcpListener;
use std::sync::Arc;

/// The served list of the parent module: (list, where each file is read from).
type Ready = Mutex<Option<Arc<(Manifest, Vec<PathBuf>)>>>;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("openomsi-upload-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn entry(path: &str, data: &[u8]) -> Entry {
    Entry { path: path.into(), size: data.len() as u64, sha256: super::super::sha256_of(data) }
}

/// OFFER → the indices wanted (or the host's refusal).
fn offer(host: SocketAddr, session: u64, m: &VehicleManifest) -> (super::super::Connection, Result<Vec<usize>, String>) {
    let (raw, mut out, mut input) = super::super::open(host, session).unwrap();
    let body = serde_json::to_vec(m).unwrap();
    writeln!(out, "OFFER {}", body.len()).unwrap();
    out.write_all(&body).unwrap();
    out.flush().unwrap();
    let r = super::super::read_reply(&mut input).map(|len| {
        let mut a = vec![0u8; len.unwrap() as usize];
        input.read_exact(&mut a).unwrap();
        serde_json::from_slice(&a).unwrap()
    });
    ((raw, out, input), r)
}

/// Offer `m`, send the wanted files out of `data` (by index) and finish: the files that came.
fn upload(host: SocketAddr, session: u64, m: &VehicleManifest, data: &[Vec<u8>]) -> (Vec<usize>, Result<Option<u64>, String>) {
    let ((raw, mut out, mut input), wanted) = offer(host, session, m);
    let wanted = wanted.unwrap();
    for k in &wanted {
        writeln!(out, "PUT {k} {}", data[*k].len()).unwrap();
        out.write_all(&data[*k]).unwrap();
    }
    writeln!(out, "DONE").unwrap();
    out.flush().unwrap();
    for _ in &wanted {
        assert_eq!(super::super::read_reply(&mut input).unwrap(), None);
    }
    let done = super::super::read_reply(&mut input);
    let _ = raw.shutdown(std::net::Shutdown::Both);
    (wanted, done)
}

/// One request on the mods connection, its answer's bytes.
fn ask(host: SocketAddr, session: u64, request: &str) -> Result<Vec<u8>, String> {
    let (raw, mut out, mut input) = super::super::open(host, session).unwrap();
    writeln!(out, "{request}").unwrap();
    out.flush().unwrap();
    let r = super::super::read_reply(&mut input).map(|len| {
        let mut a = vec![0u8; len.unwrap() as usize];
        input.read_exact(&mut a).unwrap();
        a
    });
    let _ = raw.shutdown(std::net::Shutdown::Both);
    r
}

/// `OFFER2` on a fresh connection: the plan, and the connection.
fn offer2(host: SocketAddr, session: u64, m: &VehicleManifest) -> (super::super::Connection, Vec<(usize, Vec<u64>)>) {
    let (raw, mut out, mut input) = super::super::open(host, session).unwrap();
    let plan = offer_pieces(&mut out, &mut input, &serde_json::to_vec(m).unwrap()).unwrap();
    ((raw, out, input), plan)
}

/// A player's game hosting a session keeps its own bus in its store as a server keeps one
/// sent to it - by the fingerprint its player announces, so the joining players fetch it
/// by that; a file refused for what it is is withheld, as ever.
#[test]
fn a_hosting_game_keeps_its_own_bus_for_the_others() {
    let _turn = super::super::vehicles::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store_dir = temp("own-store");
    super::super::vehicles::open_for_test(&store_dir);
    let content = temp("own-content");
    let files = [
        ("Vehicles/OwnBus/own.bus", b"[model]\nmodel\\model.cfg\nown\n".to_vec()),
        ("Vehicles/OwnBus/model/model.cfg", b"[mesh]\nx.o3d\n".to_vec()),
        ("Vehicles/OwnBus/texture/x.dat", vec![4u8; 50_000]),
        ("Vehicles/OwnBus/texture/bad.dds", b"no texture at all".to_vec()),
    ];
    let mut chosen = Vec::new();
    for (rel, data) in &files {
        let p = rel.split('/').fold(content.clone(), |p, c| p.join(c));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, data).unwrap();
        chosen.push((rel.to_string(), p));
    }
    let fp = keep_files("Vehicles/OwnBus/own.bus", chosen.clone()).unwrap();
    let m = VehicleManifest { bus: "Vehicles/OwnBus/own.bus".into(), entries: chosen.iter().map(|(r, p)| entry(r, &std::fs::read(p).unwrap())).collect() }.validated().unwrap();
    assert_eq!(fp, m.fingerprint(), "the fingerprint its player's game announces");
    assert_eq!(super::super::vehicles::manifest(&fp), Some(m));
    // mounted and served as a server would, without the refused file
    assert!(super::super::vehicles::bus_file(&fp).is_some());
    let held = super::super::vehicles::withheld_of(&fp);
    assert_eq!(held.len(), 1, "{held:?}");
    assert_eq!(held[0].path, "Vehicles/OwnBus/texture/bad.dds");
    assert!(super::super::vehicles::blob(&super::super::sha256_of(b"no texture at all")).is_none());
    assert_eq!(super::super::vehicles::blob(&super::super::sha256_of(&files[2].1)), Some(files[2].1.clone()));
    // the same bus again: kept already
    assert_eq!(keep_files("Vehicles/OwnBus/own.bus", chosen).unwrap(), fp);
    let _ = std::fs::remove_dir_all(&content);
    let _ = std::fs::remove_dir_all(&store_dir);
}

#[test]
fn an_upload_that_broke_off_goes_on_where_it_stopped() {
    let _turn = super::super::vehicles::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let key = std::sync::Arc::new(omsi_net::hostkey::HostKey::generate().unwrap());
    let session = key.session();
    let store_dir = temp("resume-up-store");
    super::super::vehicles::open_for_test(&store_dir);
    accept(true);
    let ready: Arc<Ready> = Arc::new(Mutex::new(Some(Arc::new((Manifest::default(), Vec::new())))));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let host = listener.local_addr().unwrap();
    {
        let ready = ready.clone();
        std::thread::spawn(move || {
            for s in listener.incoming().flatten() {
                let (ready, key) = (ready.clone(), key.clone());
                std::thread::spawn(move || super::super::handle(s, &key, &ready));
            }
        });
    }
    // a bus with a texture of several pieces, and a small file
    let content = temp("resume-up-content");
    let mut x = 0x2545_f491u32;
    let big: Vec<u8> = (0..(2 * super::super::pieces::PIECE + 777))
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect();
    let files = [("Vehicles/UpResume/u.bus", b"[model]\nmodel\\model.cfg\nup resume\n".to_vec()), ("Vehicles/UpResume/texture/big.dat", big.clone())];
    let mut chosen = Vec::new();
    for (rel, data) in &files {
        let p = rel.split('/').fold(content.clone(), |p, c| p.join(c));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, data).unwrap();
        chosen.push((rel.to_string(), p));
    }
    let m = VehicleManifest { bus: "Vehicles/UpResume/u.bus".into(), entries: chosen.iter().map(|(r, p)| entry(r, &std::fs::read(p).unwrap())).collect() }.validated().unwrap();
    let k_big = m.entries.iter().position(|e| e.path.ends_with("big.dat")).unwrap();
    // the first attempt: the server lacks every piece; two of the big file's go, then the
    // line breaks
    let ((raw, mut out, mut input), plan) = offer2(host, session, &m);
    assert_eq!(plan.iter().map(|(_, p)| p.len()).sum::<usize>(), 4, "{plan:?}");
    for i in [0u64, 2] {
        let (start, len) = super::super::pieces::span(big.len() as u64, i);
        let data = &big[start as usize..(start + len) as usize];
        let (codec, wire) = super::super::pieces::pack(data);
        writeln!(out, "PUTPIECE {k_big} {i} {} {codec} {}", data.len(), wire.len()).unwrap();
        out.write_all(&wire).unwrap();
    }
    out.flush().unwrap();
    for _ in 0..2 {
        assert_eq!(super::super::read_reply(&mut input).unwrap(), None);
    }
    let _ = raw.shutdown(std::net::Shutdown::Both);
    // again: only what did not come is asked for
    let ((raw, _, _), plan) = offer2(host, session, &m);
    let big_left = plan.iter().find(|(k, _)| *k == k_big).map(|(_, p)| p.clone()).unwrap();
    assert_eq!(big_left, vec![1], "the pieces that came are not sent again");
    assert_eq!(plan.len(), 2, "the small file is still wanted whole");
    let _ = raw.shutdown(std::net::Shutdown::Both);
    // the player's game finishes it, and the server keeps the bus
    assert_eq!(send_files("Vehicles/UpResume/u.bus", chosen, host, session, true), Ok(2));
    let fp = m.fingerprint();
    assert_eq!(super::super::vehicles::manifest(&fp), Some(m.clone()));
    assert_eq!(super::super::vehicles::blob(&m.entries[k_big].sha256), Some(big));
    accept(false);
    let _ = std::fs::remove_dir_all(&content);
    let _ = std::fs::remove_dir_all(&store_dir);
}

#[test]
fn a_file_the_bus_has_twice_never_ends_the_upload() {
    let _turn = super::super::vehicles::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let key = std::sync::Arc::new(omsi_net::hostkey::HostKey::generate().unwrap());
    let session = key.session();
    let store_dir = temp("twice-store");
    super::super::vehicles::open_for_test(&store_dir);
    accept(true);
    let ready: Arc<Ready> = Arc::new(Mutex::new(Some(Arc::new((Manifest::default(), Vec::new())))));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let host = listener.local_addr().unwrap();
    {
        let ready = ready.clone();
        std::thread::spawn(move || {
            for s in listener.incoming().flatten() {
                let (ready, key) = (ready.clone(), key.clone());
                std::thread::spawn(move || super::super::handle(s, &key, &ready));
            }
        });
    }
    // the same texture in two folders of the bus
    let content = temp("twice-content");
    let tex: Vec<u8> = (0..(super::super::pieces::PIECE + 4321)).map(|i| (i * 7 % 251) as u8).collect();
    let files = [("Vehicles/Twice/t.bus", b"[model]\nmodel\\model.cfg\ntwice\n".to_vec()), ("Vehicles/Twice/a/tex.dat", tex.clone()), ("Vehicles/Twice/b/tex.dat", tex.clone())];
    let mut chosen = Vec::new();
    for (rel, data) in &files {
        let p = rel.split('/').fold(content.clone(), |p, c| p.join(c));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, data).unwrap();
        chosen.push((rel.to_string(), p));
    }
    let m = VehicleManifest { bus: "Vehicles/Twice/t.bus".into(), entries: chosen.iter().map(|(r, p)| entry(r, &std::fs::read(p).unwrap())).collect() }.validated().unwrap();
    let (ka, kb) = (m.entries.iter().position(|e| e.path.ends_with("a/tex.dat")).unwrap(), m.entries.iter().position(|e| e.path.ends_with("b/tex.dat")).unwrap());
    let put = |out: &mut super::super::channel::SecureWriter<std::net::TcpStream>, k: usize, i: u64| {
        let (start, len) = super::super::pieces::span(tex.len() as u64, i);
        let data = &tex[start as usize..(start + len) as usize];
        let (codec, wire) = super::super::pieces::pack(data);
        writeln!(out, "PUTPIECE {k} {i} {} {codec} {}", data.len(), wire.len()).unwrap();
        out.write_all(&wire).unwrap();
        out.flush().unwrap();
    };
    // one connection of the player offers the bus; another finishes the texture in `a`
    let ((raw_late, mut out_late, mut in_late), plan) = offer2(host, session, &m);
    assert_eq!(plan.len(), 3);
    let ((raw, mut out, mut input), _) = offer2(host, session, &m);
    for i in 0..2 {
        put(&mut out, ka, i);
        assert_eq!(super::super::read_reply(&mut input).unwrap(), None);
    }
    let _ = raw.shutdown(std::net::Shutdown::Both);
    // a connection that offered after that: the texture in `b` is whole with it...
    let ((raw2, mut out2, mut in2), plan2) = offer2(host, session, &m);
    assert!(!plan2.iter().any(|(k, _)| *k == kb), "{plan2:?}");
    // ...and a piece of it still on its way there is taken, not the end of the upload
    put(&mut out2, kb, 0);
    assert_eq!(super::super::read_reply(&mut in2).unwrap(), None, "a piece of a file here whole already");
    put(&mut out_late, kb, 1);
    assert_eq!(super::super::read_reply(&mut in_late).unwrap(), None);
    let _ = raw2.shutdown(std::net::Shutdown::Both);
    let _ = raw_late.shutdown(std::net::Shutdown::Both);
    // the player's game the whole way: the texture goes once, the bus is kept
    assert_eq!(send_files("Vehicles/Twice/t.bus", chosen, host, session, true).map(|_| ()), Ok(()));
    assert_eq!(super::super::vehicles::manifest(&m.fingerprint()), Some(m));
    accept(false);
    let _ = std::fs::remove_dir_all(&content);
    let _ = std::fs::remove_dir_all(&store_dir);
}

#[test]
fn two_versions_of_a_bus_are_kept_side_by_side_by_their_content() {
    let _turn = super::super::vehicles::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let key = std::sync::Arc::new(omsi_net::hostkey::HostKey::generate().unwrap());
    let session = key.session();
    let store_dir = temp("store");
    super::super::vehicles::open_for_test(&store_dir);
    accept(true);
    let ready: Arc<Ready> = Arc::new(Mutex::new(Some(Arc::new((Manifest::default(), Vec::new())))));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let host = listener.local_addr().unwrap();
    {
        let ready = ready.clone();
        std::thread::spawn(move || {
            for s in listener.incoming().flatten() {
                let (ready, key) = (ready.clone(), key.clone());
                std::thread::spawn(move || super::super::handle(s, &key, &ready));
            }
        });
    }
    let bus = b"[model]\nmodel\\model.cfg\n".to_vec();
    let cfg = b"[mesh]\nbody.o3d\n".to_vec();
    let tex1 = vec![7u8; 300_000];
    let tex2 = vec![8u8; 300_000];
    let version = |tex: &Vec<u8>| -> (VehicleManifest, Vec<Vec<u8>>) {
        let m = VehicleManifest {
            bus: "Vehicles/UpTestBus/bus.bus".into(),
            entries: vec![entry("Vehicles/UpTestBus/bus.bus", &bus), entry("Vehicles/UpTestBus/model/model.cfg", &cfg), entry("Vehicles/UpTestBus/texture/body.dat", tex)],
        }
        .validated()
        .unwrap();
        // (the data in the list's order)
        let data = m.entries.iter().map(|e| if e.path.ends_with(".bus") { bus.clone() } else if e.path.ends_with(".cfg") { cfg.clone() } else { tex.clone() }).collect();
        (m, data)
    };
    // one player's version - asked for by another player before it has come: the answer
    // goes the moment it is kept, not when that player asks again
    let (m1, d1) = version(&tex1);
    let early = {
        let fp = m1.fingerprint();
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            (ask(host, session, &format!("MANIFEST {fp}")), t0.elapsed())
        })
    };
    std::thread::sleep(std::time::Duration::from_millis(300));
    let (wanted, done) = upload(host, session, &m1, &d1);
    let (answer, took) = early.join().unwrap();
    let got: VehicleManifest = serde_json::from_slice(&answer.unwrap()).unwrap();
    assert_eq!(got, m1);
    assert!(took < std::time::Duration::from_millis(1900), "answered when it came, not at the end of the wait ({took:?})");
    assert_eq!(wanted.len(), 3);
    assert_eq!(done, Ok(Some(3)));
    let fp1 = m1.fingerprint();
    assert_eq!(super::super::vehicles::manifest(&fp1), Some(m1.clone()));
    // another player's version of the same mod (same paths, another texture): only what
    // differs is wanted - and it does not take the place of the first
    let (m2, d2) = version(&tex2);
    let (wanted, done) = upload(host, session, &m2, &d2);
    assert_eq!(wanted.len(), 1, "the files both versions share are kept once");
    assert_eq!(done, Ok(Some(1)));
    let fp2 = m2.fingerprint();
    assert_ne!(fp1, fp2);
    // each one mounted as itself: the texture of each version where its bus is
    for (fp, tex) in [(&fp1, &tex1), (&fp2, &tex2)] {
        let bus_file = super::super::vehicles::bus_file(fp).unwrap();
        let body = omsi_cfg::resolve_path(bus_file.parent().unwrap(), "texture/body.dat");
        assert_eq!(&omsi_cfg::vfs::read(&body).unwrap(), tex, "{fp}");
    }
    // asked for by its content, over the wire
    let got: VehicleManifest = serde_json::from_slice(&ask(host, session, &format!("MANIFEST {fp2}")).unwrap()).unwrap();
    assert_eq!(got, m2);
    assert_eq!(ask(host, session, &format!("BLOB {}", super::super::sha256_of(&tex2))).unwrap(), tex2);
    assert!(ask(host, session, &format!("BLOB {}", "0".repeat(64))).is_err());
    assert!(ask(host, session, &format!("MANIFEST v1-{}", "0".repeat(64))).is_err());
    // nothing plain in the store
    let all: Vec<PathBuf> = std::fs::read_dir(&store_dir).unwrap().flatten().map(|e| e.path()).collect();
    assert!(!all.iter().any(|p| std::fs::read(p).map(|b| b == tex1 || b == tex2).unwrap_or(false)), "a plain file in the store");
    // still there after a restart
    super::super::vehicles::open_for_test(&store_dir);
    assert!(super::super::vehicles::manifest(&fp1).is_some() && super::super::vehicles::manifest(&fp2).is_some());
    // (not on the list every joining game fetches: they ask for it by its fingerprint)
    assert!(ready.lock().unwrap().as_ref().unwrap().0.entries.is_empty());
    // a list with a program in it is refused whole
    let mut bad = m1.clone();
    bad.entries.push(entry("Vehicles/UpTestBus/setup.exe", b"MZ"));
    let ((raw, _, _), r) = offer(host, session, &bad);
    assert!(r.unwrap_err().contains("setup.exe"));
    let _ = raw.shutdown(std::net::Shutdown::Both);
    // a file that is not what its name says is refused - and the bus is kept without it,
    // the others told so with its list (one bad texture costs a texture, not the bus)
    let fake = b"this is no texture at all, whatever its name".to_vec();
    let bad_bus = b"[model]\nbad\n".to_vec();
    let mb = VehicleManifest { bus: "Vehicles/Bad/bad.bus".into(), entries: vec![entry("Vehicles/Bad/bad.bus", &bad_bus), entry("Vehicles/Bad/t.dds", &fake)] }.validated().unwrap();
    let ((raw, mut out, mut input), wanted) = offer(host, session, &mb);
    assert_eq!(wanted.unwrap().len(), 2);
    for e in &mb.entries {
        let d = if e.path.ends_with(".bus") { &bad_bus } else { &fake };
        let k = mb.entries.iter().position(|x| x.path == e.path).unwrap();
        writeln!(out, "PUT {k} {}", d.len()).unwrap();
        out.write_all(d).unwrap();
    }
    writeln!(out, "DONE").unwrap();
    out.flush().unwrap();
    let replies: Vec<Result<Option<u64>, String>> = (0..3).map(|_| super::super::read_reply(&mut input)).collect();
    let _ = raw.shutdown(std::net::Shutdown::Both);
    assert!(replies.iter().any(|r| r.as_ref().is_err_and(|e| e.contains("t.dds") && e.contains("not a picture"))), "{replies:?}");
    assert_eq!(replies[2], Ok(Some(1)), "kept with the one file that came");
    assert_eq!(super::super::vehicles::manifest(&mb.fingerprint()), Some(mb.clone()), "kept by the fingerprint of all its files");
    let answer: serde_json::Value = serde_json::from_slice(&ask(host, session, &format!("MANIFEST {}", mb.fingerprint())).unwrap()).unwrap();
    assert_eq!(answer["withheld"][0]["path"], "Vehicles/Bad/t.dds", "{answer}");
    assert!(answer["withheld"][0]["why"].as_str().unwrap().contains("not a picture"));
    // (a game before the withheld files reads the list alone: the fingerprint is the same)
    assert_eq!(serde_json::from_value::<VehicleManifest>(answer).unwrap().validated().unwrap().fingerprint(), mb.fingerprint());
    // the file reaches nobody; its bus is mounted without it
    assert!(super::super::vehicles::blob(&super::super::sha256_of(&fake)).is_none());
    let bus_file = super::super::vehicles::bus_file(&mb.fingerprint()).unwrap();
    assert!(omsi_cfg::vfs::read(&omsi_cfg::resolve_path(bus_file.parent().unwrap(), "t.dds")).is_err());
    // and so after a restart
    super::super::vehicles::open_for_test(&store_dir);
    assert!(super::super::vehicles::bus_file(&mb.fingerprint()).is_some(), "kept without it after a restart");
    assert_eq!(super::super::vehicles::withheld_of(&mb.fingerprint()).len(), 1);
    // a bus whose bus file itself is refused is never kept
    let bad_head = VehicleManifest { bus: "Vehicles/Bad/head.bus".into(), entries: vec![entry("Vehicles/Bad/head.bus", b"[model]\0\0binary")] }.validated().unwrap();
    let ((raw, mut out, mut input), wanted) = offer(host, session, &bad_head);
    assert_eq!(wanted.unwrap().len(), 1);
    writeln!(out, "PUT 0 {}", b"[model]\0\0binary".len()).unwrap();
    out.write_all(b"[model]\0\0binary").unwrap();
    writeln!(out, "DONE").unwrap();
    out.flush().unwrap();
    assert!(super::super::read_reply(&mut input).unwrap_err().contains("not a text file"));
    assert!(super::super::read_reply(&mut input).unwrap_err().contains("has not come"));
    let _ = raw.shutdown(std::net::Shutdown::Both);
    assert!(super::super::vehicles::manifest(&bad_head.fingerprint()).is_none());
    // a player's game the whole way: offered, sent with its progress, kept
    let content = temp("send-content");
    let files = [("Vehicles/SendBus/send.bus", b"[model]\nmodel\\model.cfg\n".to_vec()), ("Vehicles/SendBus/model/model.cfg", b"[mesh]\nx.o3d\n".to_vec()), ("Vehicles/SendBus/texture/x.dat", vec![3u8; 70_000])];
    let mut chosen = Vec::new();
    for (rel, data) in &files {
        let p = rel.split('/').fold(content.clone(), |p, c| p.join(c));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, data).unwrap();
        chosen.push((rel.to_string(), p));
    }
    // (its .bus is byte for byte the test bus's above: kept once already, not sent again)
    assert_eq!(send_files("Vehicles/SendBus/send.bus", chosen.clone(), host, session, false), Ok(2));
    assert!(notices().progress().is_some_and(|(done, total)| done == total && total == 70_000 + 13), "the progress reached the end");
    assert!(super::super::vehicles::manifest(&VehicleManifest { bus: "Vehicles/SendBus/send.bus".into(), entries: chosen.iter().map(|(r, p)| entry(r, &std::fs::read(p).unwrap())).collect() }.validated().unwrap().fingerprint()).is_some(), "kept by the fingerprint the player works out too");
    // again: the server has it all, nothing goes
    assert_eq!(send_files("Vehicles/SendBus/send.bus", chosen, host, session, true), Ok(0));
    // a bus whose files the server has every one of (by their content, from other buses)
    // is kept all the same: the others ask for it by its fingerprint
    let all_here = temp("all-here");
    let mut chosen = Vec::new();
    for (rel, data) in [("Vehicles/AllHere/here.bus", &bus), ("Vehicles/AllHere/model/model.cfg", &cfg), ("Vehicles/AllHere/texture/body.dat", &tex1)] {
        let p = rel.split('/').fold(all_here.clone(), |p, c| p.join(c));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, data).unwrap();
        chosen.push((rel.to_string(), p));
    }
    let fp_here = VehicleManifest { bus: "Vehicles/AllHere/here.bus".into(), entries: chosen.iter().map(|(r, p)| entry(r, &std::fs::read(p).unwrap())).collect() }.validated().unwrap().fingerprint();
    assert!(super::super::vehicles::manifest(&fp_here).is_none());
    assert_eq!(send_files("Vehicles/AllHere/here.bus", chosen, host, session, true), Ok(0));
    assert!(super::super::vehicles::manifest(&fp_here).is_some(), "a bus of files the server had is kept by its fingerprint");
    let got: VehicleManifest = serde_json::from_slice(&ask(host, session, &format!("MANIFEST {fp_here}")).unwrap()).unwrap();
    assert_eq!(got.fingerprint(), fp_here);
    let _ = std::fs::remove_dir_all(&all_here);
    let _ = std::fs::remove_dir_all(&content);
    // switched off: refused
    accept(false);
    let ((raw, _, _), wanted) = offer(host, session, &m1);
    assert!(no_uploads(&crate::mods::failure::Failure::from(wanted.unwrap_err())));
    let _ = raw.shutdown(std::net::Shutdown::Both);
    let _ = std::fs::remove_dir_all(&store_dir);
}

/// A stock bus of an OMSI 2 installation (`OMSI_ROOT`): its fingerprint is the same each
/// time it is worked out, and another paint is another bus.
#[test]
fn a_bus_in_a_paint_is_one_fingerprint() {
    let Some(root) = std::env::var_os("OMSI_ROOT").map(PathBuf::from) else { return };
    // (working our bus out stops the game's read recording: the test that records takes turns)
    let _turn = super::super::vehicles::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    omsi_cfg::add_content_root(root.clone());
    let path = omsi_cfg::resolve_path(&root, "Vehicles/MAN_SD200/MAN_SD77.bus");
    let Ok(ty) = omsi_sim::VehicleType::load(&root, &path) else { return };
    let bus = "Vehicles/MAN_SD200/MAN_SD77.bus";
    let paint = |i: usize| ty.paint_schemes[i].name.clone();
    let files = |i: usize| super::super::bus_files::of_type(&root, &ty, Some(i), None);
    let (m, fp, sent) = own_manifest(&root, bus, &paint(0), || files(0)).unwrap();
    assert!(omsi_net::is_fingerprint(&fp));
    assert_eq!(own_fingerprint(bus, &paint(0)), fp);
    assert_eq!(m.entries.len(), sent.len());
    // worked out again from scratch: the same
    own_bus().forget();
    assert_eq!(own_manifest(&root, bus, &paint(0), || files(0)).unwrap().1, fp);
    // another paint
    let (_, other, _) = own_manifest(&root, bus, &paint(1), || files(1)).unwrap();
    assert_ne!(other, fp);
    assert!(own_fingerprint(bus, &paint(0)).is_empty(), "the fingerprint of the paint no longer worn");
}

#[test]
fn only_what_the_bus_read_goes() {
    // (the read recording is the game's own: a test working out a bus would stop it midway)
    let _turn = super::super::vehicles::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = temp("content");
    let original = temp("original");
    let write = |rel: &str, data: &[u8]| {
        let p = rel.split('/').fold(root.clone(), |p, c| p.join(c));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, data).unwrap();
        p
    };
    let bus = write("Vehicles/ReadBus/bus.bus", b"[model]\nmodel\\model.cfg\n");
    let model = write("Vehicles/ReadBus/model/model.cfg", b"[mesh]\n..\\..\\SharedDisplays\\matrix.o3d\n\n[texttexture]\nX\nMatrixFont\n");
    let used = write("Vehicles/ReadBus/texture/red/body.dds", b"red");
    write("Vehicles/ReadBus/texture/blue/body.dds", b"blue");
    write("Vehicles/ReadBus/variant2.bus", b"[model]\n");
    let shared = write("Vehicles/SharedDisplays/matrix.o3d", b"o3d");
    let font = write("Fonts/matrix.oft", b"[newfont]\nMatrixFont\nmatrix.bmp\nmatrix_a.bmp\n");
    let bitmap = write("Fonts/matrix.bmp", b"bmp");
    let other = write("Vehicles/OtherBus/x.cfg", b"x");
    omsi_cfg::add_content_root(root.clone());
    omsi_cfg::vfs::record_reads(true);
    for p in [&bus, &model, &used, &shared, &font, &bitmap, &other] {
        omsi_cfg::vfs::read(p).unwrap();
    }
    let got: Vec<String> = files_of(&original, "Vehicles/ReadBus/bus.bus").into_iter().map(|(r, _)| r).collect();
    omsi_cfg::vfs::record_reads(false);
    omsi_cfg::remove_content_root(&root);
    assert_eq!(
        got,
        vec!["Fonts/matrix.bmp", "Fonts/matrix.oft", "Vehicles/ReadBus/bus.bus", "Vehicles/ReadBus/model/model.cfg", "Vehicles/ReadBus/texture/red/body.dds", "Vehicles/SharedDisplays/matrix.o3d"]
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn folders_of_a_bus_and_what_it_points_into() {
    assert_eq!(owner_folder("Vehicles/MAN_SD200/model/model.cfg").as_deref(), Some("Vehicles/MAN_SD200"));
    assert_eq!(owner_folder("Sounds/Shared/x.wav").as_deref(), Some("Sounds/Shared"));
    assert_eq!(owner_folder("Fonts/x.oft"), None);
    assert_eq!(norm("Vehicles/MAN/model/../../Anzeigen/Matrix/x.o3d"), "Vehicles/Anzeigen/Matrix/x.o3d");
    assert_eq!(norm("a\\.\\b//c"), "a/b/c");
}

#[test]
fn a_request_refused_is_read_past_and_never_kept() {
    // (the budgets themselves: `budget::tests`)
    // a request refused is read past (the stream stays in step), never kept
    let mut input: &[u8] = b"0123456789NEXT";
    skip(&mut input, 10).unwrap();
    assert_eq!(input, b"NEXT");
    assert!(read_body(&mut &b"abc"[..], 5).is_err(), "bytes that never come");
    assert_eq!(read_body(&mut &b"abcdef"[..], 3).unwrap(), b"abc");
}

#[test]
fn a_remembered_hash_is_checked_as_a_file_read_now() {
    let _turn = super::super::vehicles::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = temp("remembered-program");
    let p = dir.join("tex.dds");
    std::fs::write(&p, b"MZ\x90\x00 a program called a texture").unwrap();
    let md = std::fs::metadata(&p).unwrap();
    let mtime = md.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    // remembered by a join (which hashes whatever it compares)
    let key = p.to_string_lossy().to_string();
    let mut cache = super::super::hash_cache();
    cache.insert(key.clone(), (md.len(), mtime, "ab".repeat(32)));
    super::super::save_hash_cache(&cache);
    assert!(hashed(&[("Vehicles/X/tex.dds".to_string(), p.clone())]).is_empty(), "a program never leaves");
    let mut cache = super::super::hash_cache();
    cache.remove(&key);
    super::super::save_hash_cache(&cache);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The upload memory budget covers what a connection keeps and what it builds: the offer it
/// was given (for the connection's life) and an `OFFER2`'s plan of pieces (while it is built
/// and sent). A plan that would pass the budget is told `ERR busy`; nothing of an offer is
/// held once its connection is gone.
#[test]
fn an_offer_and_its_plan_are_held_against_the_memory_budget() {
    let _turn = super::super::vehicles::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store_dir = temp("budget-store");
    super::super::vehicles::open_for_test(&store_dir);
    accept(true);
    // ten files of a GB each (claimed: nothing is sent) and the bus file
    let mut entries: Vec<Entry> = (0..10u64).map(|k| Entry { path: format!("Vehicles/Budget/t{k}.dat"), size: super::super::MAX_FILE, sha256: format!("{:064x}", k + 1) }).collect();
    entries.push(entry("Vehicles/Budget/b.bus", b"[model]\nmodel.cfg\n"));
    let m = VehicleManifest { bus: "Vehicles/Budget/b.bus".into(), entries };
    let body = serde_json::to_vec(&m).unwrap();
    let busy = format!("ERR {}", crate::mods::failure::says::BUSY);
    // a connection with its offer kept: held against the kept lists' budget while it lives
    let mut inc = Incoming::default();
    let mut out = Vec::new();
    inc.offer(&mut &body[..], &mut out, body.len() as u64).unwrap();
    assert!(out.starts_with(b"OK"), "{}", String::from_utf8_lossy(&out));
    assert_eq!(kept_lists().held(), m.memory(), "the kept offer is held against the kept lists' budget");
    assert_eq!(transfers().held(), 0, "and not against the transfers'");
    drop(inc);
    assert_eq!(kept_lists().held(), 0, "all of it back once the connection is gone");
    // the transfers' budget nearly all taken: an OFFER2 whose plan (ten files of a GB,
    // 10 240 pieces) would pass it is told busy instead of being built
    let most = transfers().hold(TRANSFER_MEMORY - (64 << 10)).unwrap();
    let mut inc = Incoming::default();
    let mut out = Vec::new();
    inc.offer_pieces(&mut &body[..], &mut out, body.len() as u64).unwrap();
    assert!(String::from_utf8_lossy(&out).starts_with(&busy), "{}", String::from_utf8_lossy(&out));
    drop(inc);
    drop(most);
    // with the budget free the same plan is built and sent
    let mut inc = Incoming::default();
    let mut out = Vec::new();
    inc.offer_pieces(&mut &body[..], &mut out, body.len() as u64).unwrap();
    assert!(out.starts_with(b"OK"), "{}", String::from_utf8_lossy(&out));
    drop(inc);
    let _ = std::fs::remove_dir_all(&store_dir);
}

/// Review of #2564 (P2): the offer a connection keeps is held apart from its transfers. A
/// file sent whole on that connection is admitted against the transfers' budget alone - with
/// that budget all but this file's bytes taken (the kept offer used to tip it over: `busy`
/// for good, as retrying cannot free the connection's own offer), and a file larger than the
/// whole budget still comes alone while the offer is kept.
#[test]
fn a_connection_keeping_its_offer_still_sends_its_files_whole() {
    let _turn = super::super::vehicles::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store_dir = temp("kept-offer-put");
    super::super::vehicles::open_for_test(&store_dir);
    accept(true);
    let data = b"a texture sent whole".to_vec();
    let bus = b"[model]\nmodel.cfg\n".to_vec();
    let m = VehicleManifest { bus: "Vehicles/Whole/b.bus".into(), entries: vec![entry("Vehicles/Whole/b.bus", &bus), entry("Vehicles/Whole/t.dat", &data)] };
    let body = serde_json::to_vec(&m).unwrap();
    let mut inc = Incoming::default();
    let mut out = Vec::new();
    inc.offer(&mut &body[..], &mut out, body.len() as u64).unwrap();
    let wanted: Vec<usize> = serde_json::from_slice(&out[out.iter().position(|b| *b == b'\n').unwrap() + 1..]).unwrap();
    let idx = m.entries.iter().position(|e| e.path.ends_with("t.dat")).unwrap();
    assert!(wanted.contains(&idx));
    assert!(kept_lists().held() > 0, "the offer is kept");
    // a file larger than the whole transfers' budget comes alone, the offer kept beside it
    drop(transfers().hold(2 * TRANSFER_MEMORY).expect("admitted alone"));
    // with all but this file's bytes taken, it is admitted all the same
    let most = transfers().hold(TRANSFER_MEMORY - data.len() as u64).unwrap();
    let mut out = Vec::new();
    inc.put(&mut &data[..], &mut out, idx, data.len() as u64).unwrap();
    assert_eq!(String::from_utf8_lossy(&out), "OK\n");
    drop(most);
    drop(inc);
    assert_eq!((kept_lists().held(), transfers().held()), (0, 0));
    let _ = std::fs::remove_dir_all(&store_dir);
}

/// Review of #2564 (P1): the copy of a list a `MANIFEST` answer takes is held against the
/// transfers' budget while it is answered (`ERR busy` when there is no room), and the answer
/// goes straight to the connection - the same bytes as the list written as JSON.
#[test]
fn a_manifest_answer_is_held_against_the_transfers_budget() {
    let _turn = super::super::vehicles::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store_dir = temp("manifest-budget");
    super::super::vehicles::open_for_test(&store_dir);
    accept(true);
    let bus = b"[model]\nmodel.cfg\n".to_vec();
    let m = VehicleManifest { bus: "Vehicles/Listed/b.bus".into(), entries: vec![entry("Vehicles/Listed/b.bus", &bus)] }.validated().unwrap();
    super::super::vehicles::put(&m.entries[0], &bus).unwrap();
    let (fp, _) = super::super::vehicles::commit(m.clone()).unwrap();
    let fp = Fingerprint::parse(&fp).unwrap();
    let all = transfers().hold(TRANSFER_MEMORY).unwrap();
    let mut out = Vec::new();
    send_manifest(&mut out, &fp).unwrap();
    assert_eq!(String::from_utf8_lossy(&out), format!("ERR {}\n", crate::mods::failure::says::BUSY));
    drop(all);
    let mut out = Vec::new();
    send_manifest(&mut out, &fp).unwrap();
    let nl = out.iter().position(|b| *b == b'\n').unwrap();
    let want = serde_json::to_vec(&m).unwrap();
    assert_eq!(String::from_utf8_lossy(&out[..nl]), format!("OK {}", want.len()));
    assert_eq!(&out[nl + 1..], &want[..], "the list as JSON, as before");
    assert_eq!(transfers().held(), 0, "given back once answered");
    let _ = std::fs::remove_dir_all(&store_dir);
}

/// Our bus's files are listed on the identifying thread, not the frame's; and a bus whose
/// files make no valid list is not worked out again every frame (only when the bus or its
/// paint changes).
#[test]
fn a_bus_that_makes_no_list_is_not_worked_out_every_frame() {
    // (working our bus out stops the game's read recording: the test that records takes turns)
    let _turn = super::super::vehicles::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = temp("no-list");
    let own: &'static OwnBus = Box::leak(Box::default());
    let frame = std::thread::current().id();
    let listed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let lister = || {
        let listed = listed.clone();
        move || {
            assert_ne!(std::thread::current().id(), frame, "listed on the frame's thread");
            listed.fetch_add(1, Ordering::Relaxed);
            Vec::new()
        }
    };
    // (a bus path that leaves the root: no valid list)
    let bus = "../outside.bus";
    own.identify(&root, bus, "paint", lister());
    let t = std::time::Instant::now();
    while !own.has_failed(bus, "paint") && t.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(own.has_failed(bus, "paint"));
    // (the thread lets go of `identifying` right after: give it the moment)
    std::thread::sleep(std::time::Duration::from_millis(50));
    for _ in 0..5 {
        own.identify(&root, bus, "paint", lister());
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(listed.load(Ordering::Relaxed), 1, "listed once, not once a frame");
    // another paint is worked out
    own.identify(&root, bus, "another", lister());
    let t = std::time::Instant::now();
    while listed.load(Ordering::Relaxed) < 2 && t.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(listed.load(Ordering::Relaxed), 2);
    let _ = std::fs::remove_dir_all(&root);
}

/// One file reached by two spellings (`ü` composed, and `u` + a combining diaeresis as macOS
/// may give it) is listed once, by the key the list is checked by: the list stays valid.
#[test]
fn one_file_by_two_unicode_spellings_is_listed_once() {
    let root = temp("spellings");
    let composed = root.join("Vehicles/B/Bahn\u{fc}bergang.dds");
    let decomposed = root.join("Vehicles/B/Bahnu\u{308}bergang.dds");
    let list = listed(&root, vec![root.join("Vehicles/B/b.bus"), composed, decomposed]);
    assert_eq!(list.len(), 2, "{list:?}");
    let _ = std::fs::remove_dir_all(&root);
}
