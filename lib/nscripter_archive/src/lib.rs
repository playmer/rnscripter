#[allow(dead_code)]
pub mod nscripter_archive {
    use core::panic;
    use std::{default, fs::File, io::{ErrorKind, Read, SeekFrom}, os::windows::fs::FileExt};

    struct FileHelper {
        file : File,
        key_table : [u8; 256]
    }

    impl FileHelper {
        fn read_buffer<const N: usize>(&mut self) -> [u8; N] {
            let mut buffer = [0u8; N];
            
            let res = self.file.read_exact(&mut buffer);
            match res {
                Err(error) if error.kind() == ErrorKind::UnexpectedEof => panic!("Unexpected end of file"),
                _ => {}
            }
            res.expect("Unexpected error during read");

            for byte in &mut buffer {
                *byte = self.key_table[*byte as usize];
            }

            return buffer;
        }

        fn read_u8(&mut self) -> u8 {
            const SIZE : usize = std::mem::size_of::<u8>();
            let buffer = self.read_buffer::<SIZE>();
            return u8::from_be_bytes(buffer);
        }
    
        fn read_u16_be(&mut self) -> u16 {
            const SIZE : usize = std::mem::size_of::<u16>();
            let buffer = self.read_buffer::<SIZE>();
            return u16::from_be_bytes(buffer);
        }
    
        fn read_u32_be(&mut self) -> u32 {
            const SIZE : usize = std::mem::size_of::<u32>();
            let buffer = self.read_buffer::<SIZE>();
            return u32::from_be_bytes(buffer);
        }

        fn read_shiftjis(&mut self) -> String {
            let mut buffer : Vec<u8> = Vec::new();
             
            loop {
                let byte = self.read_u8();
                
                if byte == 0 {
                    break;
                }
                
                buffer.push(byte);
            }
            
            use encoding_rs::SHIFT_JIS;
            let (res, _enc, errors) = SHIFT_JIS.decode(&buffer);
            if errors {
                panic!("Couldn't read a string from this file.");
            }

            return res.to_string();
        }
    }

    #[derive(Clone, Copy, Debug)] 
    pub enum Compression {
        None = 0,
        Spb = 1,
        Lzss = 2,
        Nbz = 4, // Seems to just be Bzip2
    }

    pub enum ArchiveType {
        SAR,
        NSA,
        NS2
    }

    pub struct ArchiveEntry {
        pub name : String,
        pub offset : usize,
        pub size : usize,
        decompressed_size : Option<usize>,
        pub compression : Compression
    }

    pub struct ArchiveEntryInfo {
        pub offset : usize,
        pub size : usize,
        decompressed_size : Option<usize>,
        pub compression : Compression
    }

    impl ArchiveEntry {
        pub fn info(&self) -> ArchiveEntryInfo {
            return ArchiveEntryInfo {
                offset : self.offset, 
                size : self.size, 
                decompressed_size : self.decompressed_size, 
                compression : self.compression, 
            }
        }
    }

    pub struct ArchiveIndex {
        pub entries : Vec<ArchiveEntry>,
        pub offset : usize
    }

    pub struct ArchiveReader {
        file : FileHelper,
        pub index : ArchiveIndex,
        pub archive_type : ArchiveType,
    }

    impl ArchiveReader {
        fn parse_sar_or_nsa_header(file : &mut FileHelper, archive_type : &ArchiveType, offset : u32) -> ArchiveIndex {
            let mut entries : Vec<ArchiveEntry> = Vec::new();
            let num_of_entries = file.read_u16_be();
            let file_offset = (file.read_u32_be() + offset) as usize; // Entries start at this address in the file

            println!("Number of entries: {num_of_entries}; File Offset {file_offset}");

            for _ in 0..num_of_entries {
                let name = file.read_shiftjis();
                let lowercase_name = name.to_lowercase();

                let mut compression = if matches!(archive_type, ArchiveType::NSA) {
                    match file.read_u8() {
                        0 => Compression::None,
                        1 => Compression::Spb,
                        2 => Compression::Lzss,
                        4 => Compression::Nbz,
                        _ => panic!("File is using an unknown Compression type.")
                    }
                } else {
                    Compression::None
                };

                if matches!(compression, Compression::None) {
                    if lowercase_name.ends_with(".nbz") {
                        compression = Compression::Nbz
                    } else if lowercase_name.ends_with(".spb") {
                        compression = Compression::Spb
                    }
                }

                let offset = file.read_u32_be() as usize + file_offset;
                let size = file.read_u32_be() as usize;
                let mut decompressed_size : Option<usize> = None;

                if matches!(archive_type, ArchiveType::SAR) {
                    decompressed_size = Some(size);
                } else {
                    decompressed_size = Some(file.read_u32_be() as usize);
                }
                
                // ONScripter notes decompression of these just for the sake of filling this value as a
                // large potential slowdown depending on the archive. We'll follow their lead in ignoring
                // it until the entry is actually opened.
                if matches!(compression, Compression::Nbz) || matches!(compression, Compression::Spb) {
                    decompressed_size = None;
                }

                entries.push(ArchiveEntry {
                    name : name, offset, size, decompressed_size, compression
                });
            }

            return ArchiveIndex{ entries, offset : file_offset };
        }
        
        fn parse_ns2_header(file : &mut FileHelper, offset : u32) -> ArchiveIndex {
            let mut entries : Vec<ArchiveEntry> = Vec::new();
            return ArchiveIndex{ entries, offset : 0 };
        }

        fn parse_header(file : &mut FileHelper, archive_type : &ArchiveType, offset : u32) -> ArchiveIndex {
            match (archive_type) {
                ArchiveType::SAR => return Self::parse_sar_or_nsa_header(file, archive_type, offset),
                ArchiveType::NSA => return Self::parse_sar_or_nsa_header(file, archive_type, offset),
                ArchiveType::NS2 => return Self::parse_ns2_header(file, offset)
            }
        }

        pub fn default_keytable() ->  [u8; 256] {
            let mut key_table : [u8; 256] = [0; 256];
            for i in 0..key_table.len() {
                key_table[i] = i as u8;
            }
            return key_table;
        }

        pub fn new(file : std::fs::File, archive_type : ArchiveType, offset : u32, key_table : [u8; 256]) -> ArchiveReader {
            let mut file_helper = FileHelper {file, key_table};
            let index = Self::parse_header(&mut file_helper, &archive_type, offset);

            return ArchiveReader {
                file : file_helper,
                index,
                archive_type,
            }
        }

        pub fn extract(&mut self, info : ArchiveEntryInfo) -> Vec<u8> {
            let mut buffer : Vec<u8> = Vec::new();
            
            use std::io::Seek;
            self.file.file.seek(SeekFrom::Start(info.offset as u64)).unwrap();
            buffer = vec![0; info.size];
            self.file.file.read(&mut buffer).unwrap();

            if matches!(info.compression, Compression::None) {
            } else if matches!(info.compression, Compression::Nbz) {
                //use bzip2_rs::DecoderReader;
                //let input = buffer;
                //let mut reader = DecoderReader::new(input.as_slice());
                //buffer = Vec::new();
                //std::io::copy(&mut reader, &mut buffer).unwrap();
            } else {
                //buffer = Vec::new();
            }

            return buffer;
        }
    }
}