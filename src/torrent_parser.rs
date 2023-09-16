use super::bencoded_parser::Element;
use std::collections::HashSet;
use std::{fmt,fs::File};
use super::bencoded_parser::Bencode;
use super::tracker;

#[derive(Debug)]
pub struct Torrent {
    pub announce_url: Option<String>,
    pub announce_list: Option<Vec<String>>,
    pub name: String,
    pub length: u64,
    pub info_hash: [u8; 20],
    pub piece_length: u64,
    pub peer_list: HashSet<(u32,u16)>,
    pub peer_id: [u8; 20]
}

impl Torrent {

    pub async fn parse_decoded(file: &mut File) -> Result<Torrent, InvalidTorrentFile> {

        let (decoded, info_hash) = Bencode::decode(file).unwrap();
        let (announce_url, announce_list, name, piece_length, _hashes, length) = Torrent::parse_decoded_helper(&decoded)?;

        let mut torrent = Torrent { announce_url, announce_list, name, length, info_hash, piece_length, peer_list: HashSet::new(), peer_id: [0; 20] };
        torrent = tracker::get_peers(torrent).await;

        Ok(torrent)

    }


    // Function to return Announce Url, name, piece length and hashes from a decoded torrent file
    fn parse_decoded_helper(decoded: &Element) -> Result<(Option<String>, Option<Vec<String>>, String, u64, Vec<String>, u64), InvalidTorrentFile> {

        let mut announce = None;
        let mut announce_list = None;
        let mut name = String::new();
        let mut piece_length = 0;
        let hashes = Vec::new();
        let mut length: i64 = 0;

        match decoded {
            Element::Dict(mp) => {

                // Get List of announce urls
                if mp.contains_key("announce-list") {
                    let mut tmp = Vec::new();
                    if let Element::List(l) = &mp["announce-list"] {
                        for i in l {
                            if let Element::List(l1) = i {
                                if let Element::ByteString(s) = &l1[0] {
                                    tmp.push(s.clone());
                                }
                            } 
                        }
                    }
                    announce_list = Some(tmp);
                }
                else { 
                    // Get Announce url of torrent file
                    if mp.contains_key("announce") {
                        if let Element::ByteString(s) = &mp["announce"] { announce = Some(s.clone()); } 
                        else { return Err(InvalidTorrentFile{case: 0}); }
                    } 
                    else { return Err(InvalidTorrentFile{case: 1}); }
                }

                // Get info of torrent file
                if mp.contains_key("info") {

                    match &mp["info"] {
                        Element::Dict(info_mp) => {
                            if !info_mp.contains_key("name") || !info_mp.contains_key("piece length") || !info_mp.contains_key("pieces") || (!info_mp.contains_key("length") && !info_mp.contains_key("files")) { return Err(InvalidTorrentFile{case: 6}); }

                            if let Element::ByteString(s) = &info_mp["name"] { name += s; }
                            if let Element::Integer(l) = &info_mp["piece length"] { piece_length += l; }

                            // Length for single file
                            if info_mp.contains_key("length") {
                                if let Element::Integer(l) = &info_mp["length"] { length += l; }
                            }

                            // Length for multiple files
                            if info_mp.contains_key("files") {
                                if let Element::List(files) = &info_mp["files"] {
                                    for file in files {
                                        if let Element::Dict(file_mp) = file {
                                            if let Element::Integer(l) = file_mp["length"] {
                                                length += l;
                                            }
                                        }
                                    }
                                } 
                            }

                            // Piece Hashes
                            if let Element::ByteString(s) = &info_mp["pieces"] {
                                let mut buf = Vec::new();
                                for i in s.as_bytes() {
                                    
                                    buf.push(i.clone());
                                    // if buf.len() == 20 {
                                    //     let mut hasher = Sha1::new();
                                    //     hasher.update(buf.as_mut());
                                    //     hashes.push(hasher.digest().to_string());
                                    //     buf.clear();
                                    // }
                                }
                                // println!("{:?}",buf);

                            }

                        }
                        _ => { return Err(InvalidTorrentFile{case: 3}); }
                    }
                }
                else { return Err(InvalidTorrentFile { case: 4 }); }

            },
            _ => { return Err(InvalidTorrentFile{case: 5}); }
            
        }
        Ok((announce,announce_list,name,piece_length as u64, hashes, length.abs() as u64))
    }
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