use byteorder::{WriteBytesExt, BigEndian};

use crate::torrent_parser::Torrent;

enum Message {
    KeepAlive {
        length: u32
    },
    Choke {
        length: u32
    },
    Unchoke {
        length: u32
    },
    Interested {
        length: u32
    },
    Uninterested {
        length: u32
    },
    Have {
        length: u32
    },
    BitField {
        length: u32
    },
    Request {
        length: u32
    },
    Piece {
        length: u32
    },
    Cancel {
        length: u32
    },
    Port {
        length: u32
    },
}

pub struct HandshakeMsg<'a> {
    pstrlen: u8,
    pstr: String,
    reserved: u64,
    info_hash: &'a String,
    peer_id: [u8; 20]
}

// 
impl<'a> HandshakeMsg<'a> {

    pub fn build_msg(torrent: &Torrent) -> Vec<u8> {

        let handshake = HandshakeMsg {
            pstrlen: 19,
            pstr: "BitTorrent protocol".to_string(),
            reserved: 0,
            info_hash: &torrent.info_hash,
            peer_id: torrent.peer_id
        };

        let mut buf = Vec::new();

        buf.write_u8(handshake.pstrlen).unwrap(); // pstrlen
        //pstr
        for byte in handshake.pstr.as_bytes() {
            buf.write_u8(*byte).unwrap();
        }
        buf.write_u64::<BigEndian>(handshake.reserved).unwrap(); // reserved
        // info_hash
        for byte in handshake.info_hash.as_bytes() {
            buf.write_u8(*byte).unwrap();
        }
        // peer_id
        for byte in handshake.peer_id {
            buf.write_u8(byte).unwrap();
        }

        buf
    }
}