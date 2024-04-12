use std::path::Path;

use crate::default_keytable;

// Does encoding_rs have an enum for this? Should we just use that?
pub enum Encoding {
    CP1252,
    ShiftJIS,
    Utf8
}

pub enum Obfuscation {
    None,
    Xor132, // What it says on the tin, xor every byte with 132.
    YWReturn, // Uses a table of magic bytes that, read as ascii, start with 'y', 'W', 'Carriage Return'
    KeyTable // Uses a provided key table.
}

pub fn file_name_to_decode_info(file_name : &Path) -> (Encoding, Obfuscation) {
    match file_name.to_str().unwrap() {
        "nscript.___" => {
            (Encoding::ShiftJIS, Obfuscation::KeyTable)
        }
        "nscr_sec.dat" => {
            (Encoding::ShiftJIS, Obfuscation::YWReturn)
        }
        "nscript.dat" => {
            (Encoding::ShiftJIS, Obfuscation::Xor132)
        }
        "0.utf" => {
            (Encoding::Utf8, Obfuscation::None)
        }
        "0.utf.txt" => {
            (Encoding::Utf8, Obfuscation::None)
        }
        "00.utf" => {
            (Encoding::Utf8, Obfuscation::None)
        }
        "00.utf.txt" => {
            (Encoding::Utf8, Obfuscation::None)
        }
        "pscript.dat" => {
            (Encoding::Utf8, Obfuscation::Xor132)
        }
        _ => { 
            panic!("Unknown filename, can't guess it's encoding or obfuscation scheme.")
        }
    }
}

fn decode_xor132(data : &mut Vec<u8>) {
    for byte in data.iter_mut() {
        *byte ^= 132;
    }
}

fn decode_ywreturn(data : &mut Vec<u8>) {
    let magic : [u8; 5] = [ 121, 87, 13, 128, 4 ];

    for (i, byte) in data.iter_mut().enumerate() {
        *byte ^= magic[i % 5];
    }
}

fn decode_keytable(data : &mut Vec<u8>, key_table : &[u8; 256]) {
    for byte in data.iter_mut() {
        *byte = key_table[*byte as usize];
    }
}

//// Returns a 
pub fn decode_script(data : Vec<u8>, encoding : Encoding, obfuscation : Obfuscation, key_table : &[u8; 256]) -> String {
    let mut data = data;
    match obfuscation {
        Obfuscation::Xor132 => {
            decode_xor132(&mut data)
        }
        Obfuscation::YWReturn => {
            decode_ywreturn(&mut data)
        }
        Obfuscation::KeyTable => {
            decode_keytable(&mut data, key_table)
        }
        _ => {
            // Don't need to deobfuscate data.
        }
    }

    match encoding {
        Encoding::ShiftJIS => {
            use encoding_rs::SHIFT_JIS;
            let (res, _enc, errors) = SHIFT_JIS.decode(&data);
            if errors {
                panic!("Couldn't read a string from this file.");
            }

            return res.to_string();
        }
        _ => {
            use encoding_rs::UTF_8;

            let (res, _enc, errors) = UTF_8.decode(&data);
            if errors {
                panic!("Couldn't read a string from this file.");
            }
        
            return res.to_string();
        }
    }
}

pub fn decode_script_file(name : &str) -> String {
    let file_path = Path::new(name);
    let (encoding, obfuscation) = file_name_to_decode_info(&file_path);

    let file_data = std::fs::read(&file_path).unwrap();
    decode_script(file_data, encoding, obfuscation, &default_keytable())
}
