use core::panic;

pub mod archive;
pub mod script;
pub mod image;

pub fn default_keytable() ->  [u8; 256] {
    let mut key_table : [u8; 256] = [0; 256];
    for (i, val) in key_table.iter_mut().enumerate() {
        *val = i as u8;
    }
    key_table
}

pub fn create_keytable(file : &str) ->  [u8; 256] {
    let buffer = std::fs::read(file).unwrap();
    let mut table : [u8; 256] = [0; 256];
    let mut found_table = false;

    for i in 0..buffer.len() {
        found_table = false;
        'sequence: for (table_index, buffer_index) in (i..buffer.len()).enumerate() {
            let byte_read : u8 = buffer[buffer_index]; 
            for table_val in table {
                if table_val == byte_read {
                    break 'sequence;
                }
            }
            table[table_index] = byte_read;
        }

        if found_table {
            break;
        }
    }

    if !found_table {
        panic!("Couldn't find a table in the key file!")
    }

    table
}
