//! Where `openbus` leads now (`omsi_net::official::resolve`), and the server's status there.
fn main() {
    match omsi_net::official::resolve() {
        Ok(url) => {
            println!("openbus -> {url}");
            match omsi_net::ws::query(omsi_net::official::ALIAS, false) {
                Ok(i) => println!("status: {} ({} / {} players)", i.name, i.players, i.max_players),
                Err(e) => println!("status: {e}"),
            }
        }
        Err(e) => println!("openbus: {e}"),
    }
}
