use tokio::{net::UdpSocket, time::timeout};
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};
use url::{Url, Host};
use super::torrent_parser::Torrent;

struct Request {
    connection_id: u64,
    action: u32,
    transaction_id: u32,
    info_hash: [u8; 20],
    peer_id: [u8; 20],
    downloaded: u64,
    left: u64,
    uploaded: u64,
    event: u32, // 0: none; 1: completed; 2: started; 3: stopped
    ip_addr: u32, // 0 default
    key: u32, // random
    num_want: i32, //-1 defualt
    port: u16 // Official spec says port number should be between 6881 and 6889
}

impl Request {
    fn to_buf(&self) -> Vec<u8> {

        let mut buf = Vec::new();

        buf.write_u64::<BigEndian>(self.connection_id).unwrap(); //Connection id
        buf.write_u32::<BigEndian>(self.action).unwrap(); // Action
        buf.write_u32::<BigEndian>(self.transaction_id).unwrap(); // Transaction id
        for byte in self.info_hash { buf.write_u8(byte).unwrap(); } // Info hash
        for byte in self.peer_id { buf.write_u8(byte).unwrap(); } // Peer id
        buf.write_u64::<BigEndian>(self.downloaded).unwrap(); // downloaded
        buf.write_u64::<BigEndian>(self.left).unwrap(); // left
        buf.write_u64::<BigEndian>(self.uploaded).unwrap(); // uploaded
        buf.write_u32::<BigEndian>(self.event).unwrap(); // event
        buf.write_u32::<BigEndian>(self.ip_addr).unwrap(); // ip_addr
        buf.write_u32::<BigEndian>(self.key).unwrap(); // key
        buf.write_i32::<BigEndian>(self.num_want).unwrap();
        buf.write_u16::<BigEndian>(self.port).unwrap();
        
        buf
    }
}

#[derive(Debug)]
struct Response {
    action: u32,
    _transaction_id: u32,
    _interval: u32,
    _leechers: u32,
    seeders: u32,
    peer_list: Vec<(u32,u16)>
}

// Generate a random peer id
pub fn gen_random_id() -> [u8; 20] {

    let mut buf: [u8; 20] = [0;20];
    for i in  0..20 {
        buf[i] = rand::random();
    }

    buf

}

// Function to build a request for announce
fn build_announce_req(conn_id: u64, info_hash: &[u8; 20], length: &u64, peer_id:&[u8;20] ) -> Vec<u8> {

    let req = Request {
        connection_id: conn_id,
        action: 1,
        transaction_id: rand::random(),
        info_hash: info_hash.clone(),
        peer_id: *peer_id,
        downloaded: 0,
        left: *length,
        uploaded: 0,
        event: 0,
        ip_addr: 0,
        key: rand::random(),
        num_want: -1,
        port: 6881 // 6881 - 6889
    };

    req.to_buf()
}

// Return Initial Connection request buffer
fn build_connection_req() -> Vec<u8> {

    let mut buf:Vec<u8> = Vec::new();

    // Connection id
    buf.write_u64::<BigEndian>(0x41727101980).unwrap();

    // action
    buf.write_u32::<BigEndian>(0).unwrap();

    // transaction id
    let transaction_id: u32 = rand::random();
    buf.write_u32::<BigEndian>(transaction_id).unwrap();

    buf

}

// Convert Url into connect format
fn parse_url(announce_url: &String) -> (String, String) {

    let parsed_url = Url::parse(&announce_url).unwrap();
    let mut remote_addr = String::new();
    if let Host::Domain(s) = parsed_url.host().unwrap() {
        remote_addr.push_str(s);
    }
    remote_addr.push(':');
    remote_addr.push_str(parsed_url.port().unwrap().to_string().as_mut());

    // let fin_addr = remote_addr.to_socket_addrs().unwrap().next().unwrap();

    (remote_addr, parsed_url.path().to_owned())
}

// Return action, transaction id, and connection id
fn parse_connection_resp(mut buf: &[u8]) -> (u32, u32, u64) {
    (
        buf.read_u32::<BigEndian>().unwrap(), //action
        buf.read_u32::<BigEndian>().unwrap(), //transaction_id
        buf.read_u64::<BigEndian>().unwrap()  //connection_id
    )
}

// Parse response of announce request
fn parse_announce_resp(mut buf: &[u8]) -> Response {

    let mut parsed = Response { 
        action: buf.read_u32::<BigEndian>().unwrap(),
        _transaction_id: buf.read_u32::<BigEndian>().unwrap(), 
        _interval:buf.read_u32::<BigEndian>().unwrap(),
        _leechers: buf.read_u32::<BigEndian>().unwrap(), 
        seeders: buf.read_u32::<BigEndian>().unwrap(),
        peer_list: Vec::new()
    };

    for _ in 0..parsed.seeders {
        let ip = buf.read_u32::<BigEndian>().unwrap();
        let port = buf.read_u16::<BigEndian>().unwrap();
        parsed.peer_list.push((ip,port));
    }

    parsed

}

async fn peer_list_helper(torrent: &Torrent, announce_url: &String, socket: &UdpSocket) -> Option<Vec<(u32,u16)>> {

    let (remote_addr, _path) = parse_url(announce_url);

    // Connect to remote addr
    println!("{remote_addr}");

    if let Ok(()) = socket.connect(&remote_addr).await {
        println!("Connected to {remote_addr}");
    }
    else {
        return None;
    }

    let mut res:[u8; 16] = [0; 16];
    let connect_request = build_connection_req();
    

    // Send Connection request
    if let Ok(_) = socket.send(&connect_request).await {
        println!("Connection request sent");
    }
    else {
        println!("Unable to send connection request");
        return None;
    }

    // Recieve intital response
    if let Ok(bytes_read) = timeout(tokio::time::Duration::from_secs(6),socket.recv(&mut res)).await {
        
        if let Ok(b) = bytes_read {
            println!("Initial Response recieved {b}");
        }
        else {
            println!("Connection request refused");
            return None;
        }

    }
    else {
        println!("Response not recieved");
        return None;
    }

    // Parse Initial Response
    let (action, _, connection_id) = parse_connection_resp(&res);
    dbg!(action);
    
    let mut res = [0; 8192];
    let announce_req = build_announce_req(connection_id, &torrent.info_hash, &torrent.length, &torrent.peer_id);
    
    for t in 0..8 {
        // Make announce request
        socket.send(&announce_req).await.unwrap();
        println!("Announce request sent {t}th time");

        // Recieve Announce Response
        println!("Waiting for Response");
        if let Ok(bytes_read) = timeout(tokio::time::Duration::from_secs((2u64.pow(t)) * 15),socket.recv(&mut res)).await {
            println!("Announce Response recieved {}",bytes_read.unwrap());
            break;
        }
    }
    
    
    // Parse Announce Response
    let resp = parse_announce_resp(&mut res);
    // println!("{:#?}",resp);
    dbg!(resp.action);

    Some(resp.peer_list)

}


// Function to get peer list
pub async fn get_peers(mut torrent: Torrent) -> Torrent {

    // Assign peer id
    torrent.peer_id = gen_random_id();

    // Create udp socket
    let socket = UdpSocket::bind("0.0.0.0:8080").await.unwrap();

    println!();

    // Check for announce_url and announce_list
    if let Some(announce_url) = &torrent.announce_url {
        
        if let Some(peers) = peer_list_helper(&torrent, announce_url, &socket).await {
            for peer in peers {
                torrent.peer_list.insert(peer);
            }
        }

    } else if let Some(announce_list) = &torrent.announce_list {

        for announce_url in announce_list {
            
            // println!("{announce_url}");
            if announce_url[0..=5].as_bytes() != "udp://".as_bytes() { continue; } 

            if let Some(peers) = peer_list_helper(&torrent, &announce_url, &socket).await {

                println!("Recieved {} peers", peers.len());

                for peer in peers {
                    torrent.peer_list.insert(peer);
                }
                println!("Total peers: {}", torrent.peer_list.len());
            }
            else {
                println!("Recieved None peers");
            }
            println!();
        }

    }
    
    torrent    
}