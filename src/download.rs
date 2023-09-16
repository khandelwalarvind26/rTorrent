use std::net::{Ipv4Addr, SocketAddrV4};
use tokio::io::{AsyncWriteExt, AsyncReadExt};
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



async fn connect(peer: (u32,u16), info_hash: [u8; 20], peer_id: [u8;20]) {

    let socket = SocketAddrV4::new(Ipv4Addr::from(peer.0),peer.1);
    println!("Connecting to {socket}");
    let res = timeout(tokio::time::Duration::from_secs(2),TcpStream::connect(socket)).await;
    match res {

        Ok(socket) => {
            
            match socket {
                Ok(stream) => {
                    println!("Connected");
                    handle_connection(stream, info_hash, peer_id).await;
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


async fn handle_connection(mut stream: TcpStream, info_hash: [u8; 20], peer_id: [u8;20]) {

    // Get handshake msg
    let mut handshake_msg = HandshakeMsg::build_msg(info_hash, peer_id);

    // Write handshake message to stream
    stream.write(&mut handshake_msg).await.unwrap();

    // Read Handshake response
    let mut buf = [0; 1028];

    match stream.read(&mut buf).await {
        Ok(bytes_read) => {
            println!("Read {bytes_read} bytes");
        },
        Err(_e) => {
            // println!("Error in reading: {e}");
        }
    }
}