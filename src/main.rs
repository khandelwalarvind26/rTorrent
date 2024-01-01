use std::{fs::File,env, sync::Arc};
use torrent_rust::{
    torrent_parser::Torrent,
    download,
    tracker::get_peers
};
use tokio;

#[tokio::main]
async fn main() {
    
    // Open file and get decoded and info hash
    let mut args = env::args();
    args.next();

    let mut dir = env::current_dir().unwrap();
    dir = dir.join(args.next().unwrap_or("Enter Arguments".to_string()));
    let mut file = File::open(dir).unwrap();
    
    // All info mentioned in torrent file
    let mut torrent = Torrent::parse_decoded(&mut file).await.unwrap(); 
    
    let file = File::create("/home/arvind/Downloads/".to_string() + &torrent.name.to_owned()).unwrap();

    let (announce_url, announce_list) = (torrent.announce_url, torrent.announce_list);
    (torrent.announce_url, torrent.announce_list) = (None, None);

    // Get peers
    let h1 = get_peers(
        torrent.info_hash.clone(),
        torrent.length.clone(),
        torrent.peer_id.clone(),
        announce_url,
        Arc::clone(&torrent.peer_list),
        announce_list
    );

    let h3 = download::download_print(Arc::clone(&torrent.downloaded), torrent.length.clone());
    
    // Download torrent
    let h2 = download::download_file(torrent, file);
    

    tokio::join!(h1, h2, h3);



}