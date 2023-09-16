use std::net::{Ipv4Addr, SocketAddrV4};
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::net::TcpStream;
use super::torrent_parser::Torrent;
use super::message::HandshakeMsg;
// Have message -> multiple have messages each have piece index
// bitfield message -> 01101 have 1,2,4 pieces



pub async fn download(torrent: &Torrent) {    
    for peer in &torrent.peer_list {
        let socket = SocketAddrV4::new(Ipv4Addr::from(peer.0),peer.1);
        if let Ok(stream) = TcpStream::connect(socket).await {
            handle_connection(stream, &torrent).await;
        }
    }
}

async fn handle_connection(mut stream: TcpStream, torrent: &Torrent) {

    // Get handshake_msg
    let mut handshake_msg = HandshakeMsg::build_msg(torrent);

    // Write handshake message to stream
    stream.write(&mut handshake_msg).await.unwrap();

    // Read Handshake response
    let mut buf = [0; 1028];
    stream.read(&mut buf).await.unwrap();
}