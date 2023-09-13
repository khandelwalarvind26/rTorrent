use std::fs::File;
use std::fmt;
use torrent_rust::bencoded_parser::{Bencode, Element};


struct _Torrent {
    announce_url: String,
    length: i64,
    name: String,
    piece_length: i64,
    info_hash: String
}

fn main() {
    
    let mut file = File::open("/home/arvind/Downloads/abc.torrent").unwrap();
    let (decoded, info_hash) = Bencode::decode(&mut file).unwrap();
    
    let (announce_url, name, piece_length) = parse_decoded(&decoded).unwrap(); 

    println!("Announce Url: {announce_url}\nName: {name}\nHash: {info_hash}\nPiece Length: {piece_length}");

}

fn parse_decoded(decoded: &Element) -> Result<(String, String, i64), InvalidTorrentFile> {

    let mut announce = String::new();
    let mut name = String::new();
    let mut piece_length = 0;

    match decoded {
        Element::Dict(mp) => {

            // Get name of torrent file
            if mp.contains_key("announce") {
                if let Element::ByteString(s) = &mp["announce"] { announce += s; } 
                else { return Err(InvalidTorrentFile{case: 0}); }
            } 
            else { return Err(InvalidTorrentFile{case: 1}); }

            // Get info of torrent file
            if mp.contains_key("info") {

                match &mp["info"] {
                    Element::Dict(info_mp) => {
                        if !info_mp.contains_key("name") || !info_mp.contains_key("piece length") { return Err(InvalidTorrentFile{case: 6}); }

                        if let Element::ByteString(s) = &info_mp["name"] { name += s; }
                        // if let Element::Integer(l) = &info_mp["length"] { length += l; }
                        if let Element::Integer(l) = &info_mp["piece length"] { piece_length += l; }
                    }
                    _ => { return Err(InvalidTorrentFile{case: 3}); }
                }
            }
            else { return Err(InvalidTorrentFile { case: 4 }); }

        },
        _ => { return Err(InvalidTorrentFile{case: 5}); }
        
    }
    Ok((announce,name,piece_length))
}



#[derive(Debug)]
pub struct InvalidTorrentFile {
    case: i32
}

impl fmt::Display for InvalidTorrentFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Keys missing in torrent file {}", self.case)
    }
}