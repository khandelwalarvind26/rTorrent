use std::net::{Ipv4Addr, SocketAddrV4};
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use byteorder::{BigEndian, ReadBytesExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use super::torrent_parser::Torrent;
use super::message::HandshakeMsg;
// Have message -> multiple have messages each have piece index
// bitfield message -> 01101 have 1,2,4 pieces


pub async fn download_file(torrent: Torrent) {    

    let mut handles = vec![];

    for peer in torrent.peer_list {

        let h = tokio::spawn( async move{
            connect(peer, torrent.info_hash, torrent.peer_id).await;
        });

        handles.push(h);
        
    }

    for handle in handles {
        handle.await.unwrap();
    }
}



async fn connect(peer: (u32,u16), info_hash: [u8; 20], peer_id: [u8; 20]) {

    let socket = SocketAddrV4::new(Ipv4Addr::from(peer.0),peer.1);
    println!("Connecting to {socket}");
    let res = timeout(tokio::time::Duration::from_secs(2),TcpStream::connect(socket)).await;
    match res {

        Ok(socket) => {
            
            match socket {
                Ok(stream) => {
                    println!("Connected");
                    handshake(stream, info_hash, peer_id).await;
                },
                Err(e) => {
                    println!("{e}");
                }
            }

        },
        _ => {}
    }

    // println!();

}


async fn handshake(mut stream: TcpStream, info_hash: [u8; 20], peer_id: [u8;20]) {

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
                        handle_connection(stream).await;
                    }
                    else {
                        println!("Terminating: Response not handshake");
        
                    }
                },

                Err(err) => {
                    println!("Error: {err}");
    
                }
            }

        },
        _ => {
            println!("Handshake response timed out");
        }

    }

}


async fn handle_connection(mut stream: TcpStream) {
    
    let mut first_iter = true;

    loop {
        let mut buf  = [0; 4];

        // Read Message and length
        let res = timeout(tokio::time::Duration::from_secs(120),stream.read_exact(&mut buf)).await;
        match res {
            Ok(resp) => {
                match resp {
                    Ok(_bytes_read) => {
                        // println!("First repsonse bytes read : {}", bytes_read);
                    },
                    Err(err) => {
                        println!("{err}");
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

        let msg = on_whole_msg(&mut stream, len).await;
        
        // // Read id of message
        let mut id = None;
        if len >= 1 {
            id = Some(msg[0]);

        }

        // // check if message is bitfield message
        let mut bitfield = None;
        if first_iter {
            first_iter = false;
            if len > 1 && id == Some(5) {
                let mut tmp = vec![];
                for i in 1..len {
                    tmp.push(msg[i as usize]);
                }
                bitfield = Some(tmp);
                continue;
            }
            
        }


        match len {
            0 => {
                //keep-alive
            },
            1 => {

                // choke/unchoke/interested/not_interested
                match id {
                    Some(0) => {
                        //choke
                    },
                    Some(1) => {
                        //unchoke
                    },
                    Some(2) => {
                        //interested
                    },
                    Some(3) => {
                        // not interested
                    },
                    _ => {
                        //invalid resp
                    }
                }

            },
            5 => {
                //have
            },
            _ => {

            }
        }
        break;
    }

}


async fn on_whole_msg(stream: &mut TcpStream, len: u32) -> Vec<u8> {

    let mut ret = Vec::new();
    while ret.len() < len as usize {
        let mut buf = [0];
        timeout(tokio::time::Duration::from_secs(10),stream.read_exact(&mut buf)).await.unwrap().unwrap();
        ret.push(buf[0]);
    }
    ret
}