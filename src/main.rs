use std::{fs::File,env};
use torrent_rust::torrent_parser::Torrent;
use tokio;
use torrent_rust::tracker;

#[tokio::main]
async fn main() {
    
    // Open file and get decoded and info hash
    let mut args = env::args();
    args.next();
    let mut dir = env::current_dir().unwrap();
    dir = dir.join(args.next().unwrap());
    let mut file = File::open(dir).unwrap();
    
    // Get announce url, name piece length, and hashes of pieces
    let torrent = Torrent::parse_decoded(&mut file).unwrap(); 

    // Show all info
    dbg!(&torrent);

    let peers = tracker::get_peers(torrent).await;
    println!("{:?}",peers);
}