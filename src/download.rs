use std::{
    fs::File,
    net::{Ipv4Addr, SocketAddrV4},
    os::unix::fs::FileExt,
    sync::Arc,
    io::{Write, stdout}, collections::HashSet
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
    torrent_parser::Torrent, 
    message::{HandshakeMsg, Message}, 
    helpers::{self, BLOCK_SIZE, CONN_LIMIT, on_whole_msg}
};

pub async fn download_file(torrent: Torrent, file: File) {    

    let mut handles = vec![];
    let file_ref = Arc::new(Mutex::new(file));
    let no_blocks = torrent.no_blocks;

    loop {
        if *(torrent.downloaded.lock().await) == torrent.length {
            break;
        }

        while (*(torrent.connections.lock().await)).len() as u32 >= CONN_LIMIT || torrent.peer_list.lock().await.is_empty() {}
        let mut q = torrent.peer_list.lock().await;
        let peer = (*q).pop_front().unwrap();

        let freq_ref: Arc<Mutex<Vec<(u16, Vec<bool>)>>> = Arc::clone(&torrent.piece_freq);
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
                handle_connection(stream, freq_ref, file_ref, no_blocks, down_ref).await;
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
                    else {
                        // dbg!("Terminating: Response not handshake");
                        None
        
                    }
                },

                Err(_) => {
                    // dbg!(err);
                    None
                }
            }

        },
        _ => {
            // dbg!("Handshake response timed out");
            None
        }

    }

}

async fn handle_connection(mut stream: TcpStream, freq_ref: Arc<Mutex<Vec<(u16, Vec<bool>)>>>, file_ref: Arc<Mutex<File>>, no_blocks: u64, down_ref: Arc<Mutex<u64>>) {

    let mut bitfield = vec![false; (*(freq_ref.lock().await)).len()];
    let mut choke = true;
    let mut requested: Option<usize> = None;

    loop {
        let mut buf  = [0; 4];

        // Read Message and length
        let res = timeout(tokio::time::Duration::from_secs(120),stream.read_exact(&mut buf)).await;
        match res {
            Ok(resp) => {
                match resp {
                    Ok(_bytes_read) => {},
                    Err(_) => {
                        // dbg!(err);
                        return;
                    }
                }
            },
            Err(_err) => {
                // dbg!("Terminating: Waiting for server response timed out");
                return;
            }
        }

        let len = ReadBytesExt::read_u32::<BigEndian>(&mut buf.as_ref()).unwrap();

        let mut msg = on_whole_msg(&mut stream, len).await;
        
        // Read id of message
        let mut id = None;
        if len >= 1 {
            id = Some(msg[0]);

        }

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
                    (*(freq_ref.lock().await))[piece_index as usize].0 += 1;
                    bitfield[piece_index as usize] = true;
                }

            },
            Some(5) => {

                //bitfield
                let mut freq_arr = freq_ref.lock().await;
                for i in 1..len {
                    for (j, val) in helpers::u8_to_bin(msg[i as usize]).iter().enumerate() {

                        let ind = (i as usize -1)*8 + j;
                        if ind >= bitfield.len() {
                            break;
                        }
                        bitfield[ind] = *val;
                        (*freq_arr)[ind].0 += 1;

                    }
                }

            },
            Some(6) => {
                // request
            },
            Some(7) => {

                // piece
                let buf = &mut msg.as_mut_slice()[1..].as_ref();
                let index = ReadBytesExt::read_u32::<BigEndian>(buf).unwrap();
                let begin = ReadBytesExt::read_u32::<BigEndian>(buf).unwrap();
                let offset = ((index as u64)*no_blocks + begin as u64)*(BLOCK_SIZE as u64);

                
                // let file = file_ref.lock().await;
                // for i in 9..msg.len() {
                //     file.write_at([msg[i]].as_mut(), offset).unwrap();
                // }

                let mut donwloaded = down_ref.lock().await;
                *donwloaded += (msg.len() - 9) as u64;

                if begin == (no_blocks - 1) as u32{
                    requested = None;
                }

            },
            Some(8) => {
                // cancel
            },
            Some(9) => {
                // port
            },
            _ => {
                // dbg!("Invalid response ", id);
                return;
            }
        }

        if !choke && requested == None {

            let (mut to_req, mut begin) = (None, None);
            {
                let mut freq_arr = freq_ref.lock().await;

                let mut mn = u16::MAX;
                for i in 0..(*freq_arr).len() {
                    if (*freq_arr)[i].0 < mn {

                        for j in 0..(*freq_arr)[i].1.len() {

                            if (*freq_arr)[i].1[j] == false {

                                to_req = Some(i);
                                begin = Some(j);
                                mn = (*freq_arr)[i].0;
                                break;

                            }
                        }
                        
                    }

                }

                if to_req != None {
                    (*freq_arr)[to_req.unwrap()].1[begin.unwrap()] = true;
                }
                else {return;}
            }

            if to_req != None {
                
                stream.write(&Message::build_request(to_req.unwrap() as u32, begin.unwrap() as u32, BLOCK_SIZE)).await.unwrap();
                // println!("Requested {}", to_req.unwrap());
                requested = to_req;

            }

        }

    }

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
        let speed = ((now - last) as f64) / (1048756 as f64);
        
        stdout.write_all(format!("\rDownloaded: {:.2} MB\nSpeed: {:.2} MB/s\nConnections: {}/{}", tot, speed, connections, CONN_LIMIT).as_bytes()).unwrap();
        
        stdout.execute(cursor::MoveUp(2)).unwrap();
        stdout.queue(terminal::Clear(terminal::ClearType::FromCursorDown)).unwrap();
        last = now;
        sleep(time::Duration::from_millis(1000)).await;
    }
    stdout.execute(cursor::Show).unwrap();

    println!("Done!");
}