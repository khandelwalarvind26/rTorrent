use std::fmt;
use std::{fs::File, collections::HashMap};
use std::io::prelude::*;
use sha1_smol::Sha1;

// Error type if invalid character found
type Result<T> = std::result::Result<T, InvalidCharError>;

impl fmt::Display for InvalidCharError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Invalid char found in bencoded file at index:{}, char:{}",self.index, self.curr)
    }
}

#[derive(Debug)]
pub enum Element {
    Dict(HashMap<String,Element>),
    Integer(i64),
    ByteString(String),
    List(Vec<Element>)
}

impl fmt::Display for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f,"{:#?}",self)
    }
}

#[derive(Debug)]
pub struct InvalidCharError{
    index: usize,
    curr: u8
}

#[derive(Debug)]
pub struct Bencode{
    buf: Vec<u8>,
    ind: usize,
    curr: char,
    info_ind: (i32,i32)
}

impl Bencode {

    fn new(buf: Vec<u8>) -> Bencode {

        Bencode { buf, ind: 0, curr: '\0', info_ind: (-1,-1) }

    }

    pub fn decode(f: &mut File) -> Result<(Element, String)> {

        // Create a buff reader and read the entire .torrent file into buf as bytes
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();

        // Create a new instance of Bencode and start parsing
        let mut instance = Bencode::new(buf);
        Ok(( instance.call_element().unwrap() , instance.calculate_hash() ))

    }

    fn calculate_hash(&mut self) -> String {

        // Define hahser
        let mut hasher = Sha1::new();

        // Create info string
        let mut info_string = Vec::new();
        for ind in self.info_ind.0..self.info_ind.1 {
            info_string.push(self.buf[ind as usize]);
        }

        // Update hasher and generate hash
        hasher.update(&info_string);
        hasher.digest().to_string()

    }

    pub fn encode(decoded: &Element) -> String {
        let mut encoded = String::new();
        match decoded {
            Element::ByteString(s) => {
                encoded.push_str(s.len().to_string().as_str());
                encoded.push(':');
                encoded.push_str(s.as_str());
            },
            Element::Dict(mp) => {
                encoded.push('d');
                for (s,element) in mp {
                    encoded.push_str(s.len().to_string().as_str());
                    encoded.push(':');
                    encoded.push_str(s.as_str());
                    encoded.push_str(Bencode::encode(element).as_str());
                }
                encoded.push('e');
            }, 
            Element::Integer(i) => {
                encoded.push('i');
                encoded.push_str(i.to_string().as_str());
                encoded.push('e');
            },
            Element::List(l) => {
                encoded.push('l');
                for element in l {
                    encoded.push_str(Bencode::encode(element).as_str());
                }
                encoded.push('e');
            }
        }
        return encoded;
    }


    // Match element using first character and call element to parse respective element
    fn call_element(&mut self) -> Result<Element> {

        match self.read_char() {

            'd' => self.read_dict(),
            '0'..='9' => self.read_byte_string(),
            'l' => self.read_list(),
            'i' => self.read_int(),
            // If none of the above found return invalid character
            _ => Err(InvalidCharError{index: self.ind, curr:self.get_char() as u8})

        }

    }


    // d.....e
    // .'
    // keys are only byte strings
    fn read_dict(&mut self) -> Result<Element> {

        // Create a hashmap to store the Dict
        let mut mp = HashMap::new();

        // loop until end of Dict found
        'outer: loop {

            // read extra char as read_byte_string will unread char
            self.read_char();

            // Key of the Dict is always a ByteString so first read key
            if let Element::ByteString(key) = self.read_byte_string()? {
                
                if key == "info" {
                    self.info_ind.0 = self.ind as i32;

                    let value = self.call_element()?;
                    mp.insert(key, value);

                    self.info_ind.1 = self.ind as i32;

                }
                else {

                    // parse value which can be any Element and insert key value pair in HashMap
                    let value = self.call_element()?;
                    mp.insert(key, value); 

                }

            }

            // Break if at end of dict
            if self.read_char() == 'e' {
                break 'outer;
            }

            // if char not end than unread
            self.unread_char();

        }

        Ok(Element::Dict(mp))

    }


    // 10:abcdefghij
    // .'
    fn read_byte_string(&mut self) -> Result<Element> {

        // Unread char as extra char read
        self.unread_char();
        let mut sz: u64 = 0;

        // get size of string
        while self.read_char() != ':' {
            sz *= 10;
            sz += (self.get_char() as u8 - '0' as u8) as u64;
        }

        let mut s = String::new();

        // get string
        for _ in 0..sz {
            s.push(self.read_char()); 
        }
        
        Ok(Element::ByteString(s))

    }


    // i324e
    // .'
    fn read_int(&mut self) -> Result<Element> {

        let mut fin: i64 = 0;
        let mut mult: i64 = 1;
        // read integer until end char recieved
        while self.read_char() != 'e' {
            if self.get_char() == '-' {
                mult = -1;
                continue;
            }
            fin *= 10;
            fin += (self.get_char() as u8 - '0' as u8) as i64;
        }
        fin *= mult;
        Ok(Element::Integer(fin))
    }


    // l....e
    // .'
    fn read_list(&mut self) -> Result<Element> {
        let mut v = Vec::new();

        // Read elements until end char recived
        while self.read_char() != 'e' {
            self.unread_char();
            v.push(self.call_element()?);
        }

        Ok(Element::List(v))

    }

    // Function to return next char in buffer
    fn read_char(&mut self) -> char {
        let tmp = self.buf[self.ind] as char;
        self.curr = tmp;
        self.ind += 1;
        tmp
    }

    // Return currently read char
    fn get_char(&self) -> char {
        self.curr
    }

    // Unread a character in current buffer 
    fn unread_char(&mut self) {
        self.ind -= 1;
        self.curr = self.buf[self.ind] as char;
    }
    
}


#[cfg(test)]
mod tests {

}
