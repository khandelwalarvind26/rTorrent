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
                        // Handle Torrent further from here
                        handle_connection(stream).await;
                    }
                    else {
                        println!("Terminating: Response not handshake");
        
                    }
                },

                Err(err) => {
                    println!("{err}");
    
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
        let mut buf = [0; 4];

        // Read Message and length
        let res = timeout(tokio::time::Duration::from_secs(120),stream.read_exact(&mut buf)).await;
        match res {
            Ok(resp) => {
                match resp {
                    Ok(_bytes_read) => {},
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
        println!("{:?}", buf);
        let len = ReadBytesExt::read_u32::<BigEndian>(&mut buf.as_ref()).unwrap();
        // Read id of message
        let mut id = None;
        if len >= 1 {
            let mut buf = [0];
            stream.read_exact(&mut buf).await.unwrap();
            id = Some(buf[0]);

        }

        // check if message is bitfield message
        let mut bitfield = None;
        if first_iter {
            first_iter = false;
            if len > 1 && id == Some(5) {
                bitfield = Some(vec![0; (len-1) as usize]);
                stream.read_exact(&mut bitfield.unwrap()).await.unwrap();
                continue;
            }
        }   

        println!("{len},{:?},{:?}",id,bitfield);
        // match len {
        //     0 => {
        //         //keep-alive
        //     },
        //     1 => {

        //         // choke/unchoke/interested/not_interested
        //         match id {
        //             0 => {
        //                 //choke
        //             },
        //             1 => {
        //                 //unchoke
        //             },
        //             2 => {
        //                 //interested
        //             },
        //             3 => {
        //                 // not interested
        //             },
        //             _ => {
        //                 //invalid resp
        //             }
        //         }

        //     },
        //     5 => {
        //         //have
        //     },
        //     _ => {

        //     }
        // }
        break;
    }

}