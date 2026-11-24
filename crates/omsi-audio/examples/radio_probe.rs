//! Connect to internet radio stations and say whether they decode: the rate, how much sound
//! arrived in a few seconds, the song title.
//!
//! usage: radio_probe <url> [url ...]
fn main() {
    env_logger::init();
    for url in std::env::args().skip(1) {
        let b = omsi_audio::radio::open(&url);
        std::thread::sleep(std::time::Duration::from_secs(6));
        println!(
            "{url}: {:.1} s buffered, status {:?}",
            b.buffered(),
            b.status()
        );
        b.close();
    }
}
