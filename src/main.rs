use std::{fs::{File, self, OpenOptions},env, sync::Arc, path::PathBuf};
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
    
    // Create a file vector and pass it to download function
    let mut file_vec = Vec::new();
    
    // if multiple files
    if torrent.file_list != None {

        // Create dir based on destination dir
        // Substitute for create_dir_all in future
        fs::create_dir_all(&destination_dir).unwrap();

        // Create files inside that dir
        for (path, size) in torrent.file_list.unwrap() {
            let file_path = destination_dir.join(path);
            file_vec.push((open_file(file_path), size));
        }
        torrent.file_list = None;
    }
    else {
        file_vec.push(( open_file(destination_dir), torrent.length ));
    }


    // Distribute torrent info
    let (announce_url, announce_list) = (torrent.announce_url, torrent.announce_list);
    (torrent.announce_url, torrent.announce_list) = (None, None);
    
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
    let h3 = download::download_file(torrent, file_vec);


    tokio::join!(h1, h2, h3);

}

fn open_file(path: PathBuf) -> File {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .unwrap() 
}