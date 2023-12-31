use std::{sync::Arc, collections::{VecDeque, HashSet}};
use tokio::{sync::Mutex, time::{sleep, self}};
use crate::helpers::CONN_LIMIT;

mod udp_tracker {

    use tokio::{net::UdpSocket, time::timeout};
    use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};
    use url::{Url, Host};

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

    struct Response {
        _action: u32,
        _transaction_id: u32,
        _interval: u32,
        _leechers: u32,
        seeders: u32,
        peer_list: Vec<(u32,u16)>
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
    fn parse_url(announce_url: String) -> (String, String) {

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
            _action: buf.read_u32::<BigEndian>().unwrap(),
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

    pub async fn peer_list_helper(info_hash: &[u8; 20], length: &u64, peer_id:&[u8;20], announce_url: String, port: u32 ) -> Option<Vec<(u32,u16)>> {

        let (remote_addr, _path) = parse_url(announce_url);


        // Connect to remote addr
        let socket = UdpSocket::bind("0.0.0.0:".to_string() + &port.to_string()).await.unwrap();

        if let Ok(()) = socket.connect(&remote_addr).await {
            // println!("Connected to {remote_addr}");
        }
        else {
            return None;
        }

        let mut res:[u8; 16] = [0; 16];
        let connect_request = build_connection_req();
        

        // Send Connection request
        if let Ok(_) = socket.send(&connect_request).await {
            // println!("Connection request sent");
        }
        else {
            // println!("Unable to send connection request");
            return None;
        }

        // Recieve intital response
        if let Ok(bytes_read) = timeout(tokio::time::Duration::from_secs(6),socket.recv(&mut res)).await {
            
            if let Ok(_) = bytes_read {
                // println!("Initial Response recieved {b}");
            }
            else {
                // println!("Connection request refused");
                return None;
            }

        }
        else {
            // println!("Response not recieved");
            return None;
        }

        // Parse Initial Response
        let (_, _, connection_id) = parse_connection_resp(&res);
        
        let mut res = [0; 8192];
        let announce_req = build_announce_req(connection_id, info_hash, length, peer_id);
        
        for t in 0..8 {
            // Make announce request
            socket.send(&announce_req).await.unwrap();
            // println!("Announce request sent {t}th time");

            // Recieve Announce Response
            // println!("Waiting for Response");
            if let Ok(_) = timeout(tokio::time::Duration::from_secs((2u64.pow(t)) * 15),socket.recv(&mut res)).await {
                // println!("Announce Response recieved {}",bytes_read.unwrap());
                break;
            }
        }
        
        
        // Parse Announce Response
        let resp = parse_announce_resp(&mut res);

        Some(resp.peer_list)

    }

}

mod http_tracker {

    use byteorder::{BigEndian, ReadBytesExt};
    use crate::bencoded_parser::Element;

    // use std::str;
    use crate::{
        helpers::u8_to_url,
        bencoded_parser::Bencode
    };

    fn url_parser(info_hash: [u8; 20], peer_id:[u8;20], announce_url: String, port: u32, uploaded: u64, downloaded: u64, left: u64, compact: bool, event: &str, numwant: Option<u64>) -> String {
        let mut ret = announce_url + "?" +
            "info_hash=" + &u8_to_url(info_hash.to_owned()) + 
            "&peer_id=" + &u8_to_url(peer_id.to_owned()) + 
            "&port=" + &port.to_string() +
            "&uploaded=" + &uploaded.to_string() +
            "&downloaded=" + &downloaded.to_string() +
            "&left=" + &left.to_string() +
            "&compact=" + if compact {"1"} else {"0"} +
            "&event=" + event;
        if numwant != None {
            ret.push_str(&("&numwant=".to_owned()+&numwant.unwrap().to_string()));
        }
        ret
    }

    pub async fn peer_list_helper(info_hash: &[u8; 20], length: &u64, peer_id:&[u8;20], announce_url: String, port: u32 ) -> Vec<(u32,u16)> {
        
        let request = url_parser(info_hash.to_owned(), peer_id.to_owned(), announce_url, port, 0, 0, length.to_owned(), true, "started", Some(50));

        let res = reqwest::get(request)
                        .await
                        .unwrap()
                        .bytes()
                        .await
                        .unwrap()
                        .to_vec();

        let decoded = Bencode::decode_u8(res).unwrap();

        let mut ret = Vec::new();
        let mut peers = Vec::new();

        match decoded {
            Element::Dict(d) => {
                if d.contains_key("peers".as_bytes()) {
                    match &d["peers".as_bytes()] {
                        Element::ByteString(s) => { peers = s.to_owned(); }
                        _ => {}
                    }
                }
            }
            _ => {}
        }

        for i in (0..peers.len()).step_by(6) {
            let ip = (&(peers.as_slice())[i..(i+4)]).read_u32::<BigEndian>().unwrap();
            let po = (&(peers.as_slice())[(i+4)..(i+6)]).read_u16::<BigEndian>().unwrap();
            ret.push((ip,po));
        }

        ret

    }
}

async fn peer_list_helper(info_hash: &[u8; 20], length: &u64, peer_id:&[u8;20], announce_url: String, port: u32, tor_ref: Arc<Mutex<VecDeque<(u32,u16)>>> ) {

    let res;
    if announce_url[0..=5].as_bytes() == "udp://".as_bytes() {
        res = udp_tracker::peer_list_helper(info_hash, length, peer_id, announce_url, port).await;
    }
    else {
        res = Some(http_tracker::peer_list_helper(info_hash, length, peer_id, announce_url, port).await);
    }

    if let Some(peers) = res {

        let mut tor = tor_ref.lock().await;
        for peer in peers {
            (*tor).push_back(peer);
        }

    }
}

// Function to get peer list
pub async fn get_peers(info_hash: [u8; 20], length: u64, peer_id: [u8;20], announce_url: Option<String>, peer_list: Arc<Mutex<VecDeque<(u32, u16)>>>, announce_list: Option<Vec<String>>, connections: Arc<Mutex<HashSet<(u32,u16)>>>) {

    loop {

        while (*(connections.lock().await)).len() as u32 >= CONN_LIMIT || !peer_list.lock().await.is_empty() {}

        // Create udp socket
        let mut port: u32 = 6881;
        let mut handles = vec![];

        // Check for announce_url and announce_list
        if let Some(announce_url) = announce_url.clone() {
            
            let tor_ref: Arc<Mutex<VecDeque<(u32, u16)>>> = Arc::clone(&peer_list);

            let h = tokio::spawn(async move{
                peer_list_helper(&info_hash, &length, &peer_id, announce_url, port, tor_ref).await;
            });   

            handles.push(h);
            port += 1;
            
        }

        if let Some(announce_list) = announce_list.clone() {

            for announce_url in announce_list {
                
                let tor_ref = Arc::clone(&peer_list);

                let h = tokio::spawn(async move{
                    peer_list_helper(&info_hash, &length, &peer_id, announce_url, port, tor_ref).await;
                });   

                handles.push(h);
                port += 1;
                
            }
            
        }
        
        for handle in handles {
            handle.await.unwrap();
        }

        sleep(time::Duration::from_secs(5)).await;
    }
}


