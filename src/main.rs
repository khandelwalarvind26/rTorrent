use std::{fs::File,env, sync::Arc};
use r_torrent::{
    torrent_parser::Torrent,
    download,
    tracker::get_peers
};
use tokio;

#[tokio::main]
async fn main() {
    
    // Open file and get decoded and info hash
    let mut args = env::args();
    if args.len() < 3 {
        panic!("usage: cargo run source_torrent destination_folder");
    }
    args.next();
    
    let dir = env::current_dir().unwrap();

    // Open .torrent file
    let source_dir = dir
        .join(args
                .next()
                .unwrap());
    let mut file = File::open(source_dir).unwrap();
    

    // All info mentioned in torrent file
    let mut torrent = Torrent::parse_decoded(&mut file).await.unwrap(); 
    
    // Initialize Destination file
    let destination_dir = dir
            .join(args
                .next()
                .unwrap())
            .join(&torrent.name.to_owned());
    let destination = File::create(destination_dir).unwrap();


    // Distribute torrent info
    let (announce_url, announce_list) = (torrent.announce_url, torrent.announce_list);
    (torrent.announce_url, torrent.announce_list) = (None, None);
    
    dbg!(&torrent.piece_length, &torrent.piece_freq.lock().await.len(), &torrent.length);
    
    // Get peers
    let h1 = get_peers(
        torrent.info_hash.clone(),
        torrent.length.clone(),
        torrent.peer_id.clone(),
        announce_url,
        Arc::clone(&torrent.peer_list),
        announce_list,
        torrent.connections.clone()
    );


    // Display function for downloading
    let h2 = download::download_print(Arc::clone(&torrent.downloaded), torrent.length.clone(), Arc::clone(&torrent.connections));


    // Download torrent
    let h3 = download::download_file(torrent, destination);


    tokio::join!(h1, h2, h3);

}