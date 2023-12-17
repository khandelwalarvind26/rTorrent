use std::fs::File;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::os::unix::fs::FileExt;
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use byteorder::{BigEndian, ReadBytesExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;
use super::torrent_parser::Torrent;
use super::message::{HandshakeMsg, Message};
// Have message -> multiple have messages each have piece index
// bitfield message -> 01101 have 1,2,4 pieces

static BLOCK_SIZE: u32 = 16384;

pub async fn download_file(torrent: Torrent, file: File) {    

    let mut handles = vec![];
    let file_ref = Arc::new(Mutex::new(file));
    let no_blocks = torrent.no_blocks;

    for peer in torrent.peer_list {

        let freq_ref: Arc<Mutex<Vec<(u16, Vec<bool>)>>> = Arc::clone(&torrent.piece_freq);
        let file_ref = Arc::clone(&file_ref);

        let h = tokio::spawn( async move{
            let stream = connect(peer, torrent.info_hash, torrent.peer_id).await;
            if let Some(stream) = stream {
                handle_connection(stream, freq_ref, file_ref, no_blocks).await;
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
    println!("Connecting to {socket}");
    let res = timeout(tokio::time::Duration::from_secs(2),TcpStream::connect(socket)).await;
    match res {

        Ok(socket) => {
            
            match socket {
                Ok(stream) => {
                    println!("Connected");
                    handshake(stream, info_hash, peer_id).await
                },
                Err(e) => {
                    println!("{e}");
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
                        println!("Terminating: Response not handshake");
                        None
        
                    }
                },

                Err(err) => {
                    println!("Error: {err}");
                    None
                }
            }

        },
        _ => {
            println!("Handshake response timed out");
            None
        }

    }

}


async fn handle_connection(mut stream: TcpStream, freq_ref: Arc<Mutex<Vec<(u16, Vec<bool>)>>>, file_ref: Arc<Mutex<File>>, no_blocks: u64) {

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
                    Err(err) => {
                        println!("Error: {err}");
                        return;
                    }
                }
            },
            Err(_err) => {
                println!("Terminating: Waiting for server response timed out");
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
                    for (j, val) in u8_to_bin(msg[i as usize]).iter().enumerate() {

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
                let file = file_ref.lock().await;
                let buf = &mut msg.as_mut_slice()[1..].as_ref();
                let index = ReadBytesExt::read_u32::<BigEndian>(buf).unwrap();
                let begin = ReadBytesExt::read_u32::<BigEndian>(buf).unwrap();
                let offset = ((index as u64)*no_blocks + begin as u64)*(BLOCK_SIZE as u64);
                // println!("Recieved {index}, {begin}");

                for i in 9..msg.len() {
                    file.write_at([msg[i]].as_mut(), offset).unwrap();
                }

                requested = None;

            },
            Some(8) => {
                // cancel
            },
            Some(9) => {
                // port
            },
            _ => {
                println!("Invalid {:?} response",id);
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


async fn on_whole_msg(stream: &mut TcpStream, len: u32) -> Vec<u8> {

    let mut ret = Vec::new();
    while ret.len() < len as usize {
        let mut buf = [0];
        let res = timeout(tokio::time::Duration::from_secs(20),stream.read_exact(&mut buf)).await;
        match res {
            Ok(resp) => {
                match resp {
                    Ok(_bytes_read) => {},
                    Err(err) => {
                        println!("Error: {err}");
                    }
                }
            },
            Err(_err) => {
                println!("Waiting for message timed out");
            }
        }
        ret.push(buf[0]);
    }
    ret

}

fn u8_to_bin(n: u8) -> Vec<bool> {
    let mut s = Vec::new();
    for i in (0..8).rev() {
        s.push(n&(1<<i) == 1<<i)
    }
    s
}


#[cfg(test)]
mod tests {
    use crate::download::u8_to_bin;

    #[test]
    fn u8_to_bin_test() {
        assert_eq!(vec![true, true, true, true, true, true, true, true], u8_to_bin(255));
        assert_eq!(vec![false, false, false, false, true, false, false, false], u8_to_bin(8));
        assert_eq!(vec![false, false, false, false, true, false, true, true], u8_to_bin(11));
    }
}
