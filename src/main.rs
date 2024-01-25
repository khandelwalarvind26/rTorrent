use std::{fs::{File, self, OpenOptions},env, sync::Arc, path::PathBuf};
use r_torrent::{
    torrent_parser::{Torrent, Piece},
    download,
    tracker::get_peers
};
use tokio::{sync::Mutex, time};

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
    
    let file_vec = Arc::new(file_vec);
    torrent.piece_freq = verify_file(torrent.piece_freq, file_vec.clone(), torrent.piece_hashes.clone(), torrent.downloaded.clone()).await;

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

async fn verify_file(freq_ref: Arc<Mutex<Vec<Piece>>>, file: Arc<Vec<(File,u64)>>, piece_hashes: Arc<Vec<Vec<u8>>>, downloaded: Arc<Mutex<u64>>) -> Arc<Mutex<Vec<Piece>>>  {

    println!("Checking already downloaded");

    let start = time::Instant::now();
    let freq = 
        Box::new(
            Arc::try_unwrap(freq_ref)
            .unwrap()
            .into_inner()
        );

    // let mut total: u64 = 0;
    let mut handles = Vec::new();
    
    for ind in 0..(*freq).len() {

        let mut ref1 = freq.clone();
        let downloaded = downloaded.clone();
        let (length, offset, file, hash) = ((*freq)[ind].length, (*freq)[ind].blocks[0].offset, file.clone(), (*piece_hashes)[ind].clone());

        let h = tokio::spawn(async move {
            if download::verify_piece(length, offset, file, &hash) {
                for block in &mut (*ref1)[ind].blocks {
                    (*block).is_req = true;
                }
                let mut download = downloaded.lock().await;
                *download += (*ref1)[ind].length;
            }
        });
        handles.push(h);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let freq_ref = Arc::new(Mutex::new(
        *freq
    ));

    
    let elapsed = start.elapsed();

    println!("Elapsed:{:.2?}\nChecked",elapsed);

    freq_ref

}