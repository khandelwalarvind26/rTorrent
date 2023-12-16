use std::{fs::File,env};
use torrent_rust::torrent_parser::Torrent;
use tokio;
use torrent_rust::download;

#[tokio::main]
async fn main() {
    
    // Open file and get decoded and info hash
    let mut args = env::args();
    args.next();

    let mut dir = env::current_dir().unwrap();
    dir = dir.join(args.next().unwrap_or("Enter Arguments".to_string()));
    let mut file = File::open(dir).unwrap();
    
    // All info mentioned in torrent file
    let torrent = Torrent::parse_decoded(&mut file).await.unwrap(); 

    // Download torrent
    let file = File::create("/home/arvind/Downloads/".to_string() + &torrent.name.to_owned()).unwrap();
    download::download_file(torrent, file).await;

}