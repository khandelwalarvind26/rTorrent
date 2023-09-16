use std::{fs::File,env};
use torrent_rust::torrent_parser::Torrent;
use tokio;

#[tokio::main]
async fn main() {
    
    // Open file and get decoded and info hash
    let mut args = env::args();
    args.next();
    let mut dir = env::current_dir().unwrap();
    dir = dir.join(args.next().unwrap());
    let mut file = File::open(dir).unwrap();
    
    // All info mentioned in torrent file
    let torrent = Torrent::parse_decoded(&mut file).await.unwrap(); 

    // Show all info
    dbg!(&torrent);

    // Download torrent


}