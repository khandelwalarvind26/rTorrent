use std::{
    fs::File,
    net::{Ipv4Addr, SocketAddrV4},
    os::unix::fs::FileExt,
    sync::Arc,
    io::{Write, stdout}, collections::HashSet, time::Duration
};
use crossterm::{QueueableCommand, cursor, terminal, ExecutableCommand};
use tokio::{
    io::{AsyncWriteExt, AsyncReadExt},
    net::TcpStream,
    sync::Mutex,
    time::{timeout, sleep, self}
};
use byteorder::{BigEndian, ReadBytesExt};
use crate::{
    torrent_parser::{Torrent, Piece}, 
    message::{HandshakeMsg, Message}, 
    helpers::{self, CONN_LIMIT, on_whole_msg}
};

pub async fn download_file(torrent: Torrent, file_vec: Vec<(File, u64)>) {    

    let mut handles = vec![];
    let file_ref = Arc::new(file_vec);

    loop {
        if *(torrent.downloaded.lock().await) == torrent.length {
            break;
        }

        while (*(torrent.connections.lock().await)).len() as u32 >= CONN_LIMIT || torrent.peer_list.lock().await.is_empty() {
            sleep(Duration::from_millis(1000)).await;
        }

        while !torrent.peer_list.lock().await.is_empty() {
            let mut q = torrent.peer_list.lock().await;
            let peer = (*q).pop_front().unwrap();

            let freq_ref:Arc<Mutex<Vec<Piece>>>  = Arc::clone(&torrent.piece_freq);
            let file_ref = Arc::clone(&file_ref);
            let down_ref = Arc::clone(&torrent.downloaded);
            let conn_ref = Arc::clone(&torrent.connections);

            if (*(conn_ref.lock().await)).contains(&peer) {
                continue;
            }

            let h = tokio::spawn( async move{

                let stream = connect(peer, torrent.info_hash, torrent.peer_id).await;
                if let Some(stream) = stream {
                    {
                        let mut connections = conn_ref.lock().await;
                        (*connections).insert(peer);
                    }
                    handle_connection(stream, freq_ref, file_ref, down_ref).await;
                    {
                        let mut connections = conn_ref.lock().await;
                        (*connections).remove(&peer);
                    }
                }
                else {
                    return;
                }
            });

            handles.push(h);
        }
        
        sleep(time::Duration::from_secs(5)).await;
    }

    for handle in handles {
        handle.await.unwrap();
    }

}

async fn connect(peer: (u32,u16), info_hash: [u8; 20], peer_id: [u8; 20]) -> Option<TcpStream> {

    let socket = SocketAddrV4::new(Ipv4Addr::from(peer.0),peer.1);
    // dbg!("Connecting to ",socket);
    let res = timeout(tokio::time::Duration::from_secs(2),TcpStream::connect(socket)).await;
    match res {

        Ok(socket) => {
            
            match socket {
                Ok(stream) => {
                    // dbg!("Connected");
                    handshake(stream, info_hash, peer_id).await
                },
                Err(_) => {
                    // dbg!(e);
                    None
                }
            }

        },
        _ => {
            None
        }
    }

}

async fn handshake(mut stream: TcpStream, info_hash: [u8; 20], peer_id: [u8;20]) -> Option<TcpStream> {

    // Get handshake msg
    let mut handshake_msg = HandshakeMsg::build_msg(info_hash, peer_id);

    // Write handshake message to stream
    stream.write(&mut handshake_msg).await.unwrap();

    // Read handshake response
    let mut buf = Vec::new();
    let res = timeout(tokio::time::Duration::from_secs(2),stream.read_buf(&mut buf)).await;
    
    // Handle handshake response : timeout, recieved - handshake or not?
    match res {
        Ok(result) => {
            match result {
                Ok(bytes_read) => {
                    // Check whether response handshake or not
                    if bytes_read == 64 && String::from_utf8_lossy(&buf[..=19])[1..] == "BitTorrent protocol".to_string() {
                        // Read waste 4 bytes
                        let mut tmp = [0; 4];
                        let _res = timeout(tokio::time::Duration::from_secs(2),stream.read_exact(&mut tmp)).await;
                        // Handle Torrent further from here
                        Some(stream)
                    }
                    else { None }
                }, _ => { None }
            }
        }, _ => { None }
    }

}

async fn handle_connection(mut stream: TcpStream, freq_ref: Arc<Mutex<Vec<Piece>>>, file: Arc<Vec<(File, u64)>>, down_ref: Arc<Mutex<u64>>) {

    let mut bitfield = vec![false; (*(freq_ref.lock().await)).len()];
    let mut choke = true;
    let mut requested: HashSet<u32> = HashSet::new();
    let mut piece_req: Option<usize> = None;

    loop {
        

        // Read length
        let mut msg;
        if let Some(len) = get_length(&mut stream).await {
            msg = on_whole_msg(&mut stream, len).await;
        }
        else { 
            if !requested.is_empty() {

                let mut freq = freq_ref.lock().await;
                for begin in requested {
                    (*freq)[piece_req.unwrap()].blocks[begin as usize].is_req = false;
                }

            }
            return; 
        }

        
        // Read id of message
        let mut id = None;
        if msg.len() >= 1 { id = Some(msg[0]); }

        match id {
            None => {
                // keep-alive
            },
            Some(0) => {
                // choke
                choke = true;
            },
            Some(1) => {
                // unchoke
                choke = false;
            },
            Some(2) => {
                // Interested
            },
            Some(3) => {
                // not-interested
            },
            Some(4) => {

                // have
                let piece_index = ReadBytesExt::read_u32::<BigEndian>(&mut msg.as_mut_slice()[1..].as_ref()).unwrap();
                if !bitfield[piece_index as usize] {
                    (*(freq_ref.lock().await))[piece_index as usize].ref_no += 1;
                    bitfield[piece_index as usize] = true;
                }

            },
            Some(5) => {

                //bitfield
                let mut freq_arr = freq_ref.lock().await;
                for i in 1..msg.len() {
                    for (j, val) in helpers::u8_to_bin(msg[i as usize]).iter().enumerate() {

                        let ind = (i as usize -1)*8 + j;
                        if ind >= bitfield.len() {
                            break;
                        }
                        bitfield[ind] = *val;
                        (*freq_arr)[ind].ref_no += 1;

                    }
                }

            },
            Some(6) => {
                // request
            },
            Some(7) => {

                let mut donwloaded = down_ref.lock().await;
                *donwloaded += (msg.len() - 9) as u64;

                let begin = write_to_file(msg, file.clone(), freq_ref.clone()).await;
                requested.remove(&begin);

            },
            Some(8) => {
                // cancel
            },
            Some(9) => {
                // port
            },
            _ => {
                return;
            }
        }

        if !choke && requested.is_empty() {

            (requested, piece_req) = make_request(freq_ref.lock().await, &mut stream, &bitfield).await;
            if requested.is_empty() {return;}

        }

    }

}

async fn get_length(stream: &mut TcpStream) -> Option<u32> {

    let mut buf  = [0; 4];
    let res = timeout(tokio::time::Duration::from_secs(120),stream.read_exact(&mut buf)).await;
    match res {
        Ok(resp) => {
            match resp {
                Ok(_) => {},
                _ => { return None; }
            }
        },
        _ => { return None; }
    }

    Some(ReadBytesExt::read_u32::<BigEndian>(&mut buf.as_ref()).unwrap())
}

async fn make_request(mut freq_arr: tokio::sync::MutexGuard<'_, Vec<Piece>>, stream: &mut TcpStream, bitfield: &Vec<bool> ) -> (HashSet<u32>, Option<usize>) {

    let mut to_req = None;
    let mut mn = u16::MAX;

    // Find piece with minimum nodes
    for i in 0..(*freq_arr).len() {
        if bitfield[i] && (*freq_arr)[i].ref_no < mn {

            for j in 0..(*freq_arr)[i].blocks.len() {

                if (*freq_arr)[i].blocks[j].is_req == false {

                    to_req = Some(i);
                    mn = (*freq_arr)[i].ref_no;
                    break;

                }
            }
            
        }

    }

    let mut req: HashSet<u32> = HashSet::new();
    
    if to_req != None {
        
        let ind = to_req.unwrap();

        let len = (*freq_arr)[ind].blocks.len();
        for j in 0..len {
            if (*freq_arr)[ind].blocks[j].is_req == false {
                (*freq_arr)[ind].blocks[j].is_req = true;
                stream.write(&Message::build_request(to_req.unwrap() as u32, j as u32, (*freq_arr)[ind].blocks[j].length as u32)).await.unwrap();
                req.insert(j as u32);
            }
        }
    }

    (req, to_req)
}

async fn write_to_file(mut msg: Vec<u8>, file: Arc<Vec<(File, u64)>>, freq_ref: Arc<Mutex<Vec<Piece>>>) -> u32 {
    // piece
    let buf = &mut msg.as_mut_slice()[1..].as_ref();
    let index = ReadBytesExt::read_u32::<BigEndian>(buf).unwrap();
    let begin = ReadBytesExt::read_u32::<BigEndian>(buf).unwrap();
    let offset = (*freq_ref).lock().await[index as usize].blocks[begin as usize].offset as u64;

    // Writing to file at different locations
    let mut ind: usize = 0;
    let mut length: u64 = 0;
    while length <= offset  {
        length += (*file)[ind].1;
        ind += 1;
    }
    ind -= 1;
    let available = (*file)[ind].1 - offset;
    if available < (msg.len()-9) as u64 {
        ((*file)[ind]).0.write_at(&msg[9..(9 + available) as usize], offset).unwrap();
        ((*file)[ind+1]).0.write_at(&msg[(9 + available) as usize ..], offset).unwrap();
    }
    else {
        ((*file)[ind]).0.write_at(&msg[9..], offset).unwrap();
    }
    begin
}

pub async fn download_print(downloaded: Arc<Mutex<u64>>, length: u64, connections: Arc<Mutex<HashSet<(u32,u16)>>>) {

    let mut stdout = stdout();

    stdout.execute(cursor::Hide).unwrap();

    let mut last = 0;

    loop {
        let now = *(downloaded.lock().await);
        let connections = (*(connections.lock().await)).len();
        if now == length {
            break;
        }
        let tot = (now as f64) / (1048756 as f64);
        let speed = ((now - last) as f64) / ((1048756*3) as f64);
        
        stdout.write_all(format!("\rDownloaded: {:.2} MB\nSpeed: {:.2} MB/s\nConnections: {}/{}", tot, speed, connections, CONN_LIMIT).as_bytes()).unwrap();
        
        stdout.execute(cursor::MoveUp(2)).unwrap();
        stdout.queue(terminal::Clear(terminal::ClearType::FromCursorDown)).unwrap();
        last = now;
        sleep(time::Duration::from_secs(3)).await;
    }
    stdout.execute(cursor::Show).unwrap();

    println!("Done!");
}