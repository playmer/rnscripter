#[allow(dead_code)]
pub mod nscripter_archive {
    use core::panic;
    use std::{collections::HashMap, fs::File, io::{ErrorKind, Read, SeekFrom}};

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

        fn seek(&mut self, seek : SeekFrom) {
            use std::io::Seek;
            self.file.seek(seek).unwrap();
        }

        fn read_slice(&mut self, offset : usize, size : usize) -> Vec<u8> {
            use std::io::Seek;

            let mut buffer : Vec<u8> = Vec::new();
            self.file.seek(SeekFrom::Start(offset as u64)).unwrap();
            buffer = vec![0; size];
            self.file.read(&mut buffer).unwrap();

            return buffer;
        }

        fn read_slice_through_keytable(&mut self, offset : usize, size : usize) -> Vec<u8> {
            let mut output = self.read_slice(offset, size);
            for byte in &mut output {
                *byte = self.key_table[*byte as usize];
            }

            return output;
        }
    }

    #[derive(Clone, Copy, Debug)] 
    pub enum Compression {
        None = 0,
        Spb = 1, 
        Lzss = 2, // Lempel–Ziv–Storer–Szymanski Compression
        Nbz = 4, // Bzip2 Compression
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
        pub entries_map : HashMap<String, usize>,
        pub offset : usize
    }

    pub struct ArchiveReader {
        file : FileHelper,
        pub index : ArchiveIndex,
        pub archive_type : ArchiveType,
    }

    // This has got to be the worst part of this project by far. A seemingly completely heretofore undocumented image
    // compression format that exists only within the sources of ONScripter and it's many forks.
    fn parse_spb_into_bmp(file : &mut FileHelper, offset : usize, size : usize) -> Vec<u8> {
        file.seek(SeekFrom::Start(offset as u64));
        let width = file.read_u16_be() as usize;
        let height = file.read_u16_be() as usize;
        
        let width_pad : usize = (4 - width * 3 % 4) % 4;
        let total_size : usize = (width * 3 + width_pad) * height + 54;


        let buffer = file.read_slice_through_keytable(offset + 4, size - 4);
        
        use bitbuffer::{BitReadBuffer, BitReadStream, BigEndian};
        let buffer = BitReadBuffer::new(&buffer, BigEndian);
        let mut stream = BitReadStream::new(buffer);

        let mut bmp_file = bmp_rust::bmp::BMP::new(height as i32, width as u32, None);

        let mut pixel_buffer : Vec<[u8;4]> = vec![[255; 4]; width * height];
        let mut decompression_buffer : Vec<u8> = vec![0; (width * height) + 4];

        for channel in (0..3).rev() {
            let mut c : i32 = (stream.read_int::<u8>(8).unwrap()) as i32;
            let mut i : usize = 0;
            decompression_buffer[i] = c as u8;
            i += 1;

            while i < (width * height) {
                let n : i32 = (stream.read_int::<u8>(3).unwrap()) as i32;
                let m : i32;

                if n == 0 {
                    decompression_buffer[i + 0] = c as u8;
                    decompression_buffer[i + 1] = c as u8;
                    decompression_buffer[i + 2] = c as u8;
                    decompression_buffer[i + 3] = c as u8;
                    i += 4;
                    
                    continue;
                } else if n == 7 {
                    m = (stream.read_int::<u8>(1).unwrap()) as i32 + 1;
                } else {
                    m = n + 2;
                }
                

                for _ in 0..4 {
                    if m == 8 {
                        c = (stream.read_int::<u8>(8).unwrap()) as i32;
                    } else {
                        let k : i32 = (stream.read_int::<u8>(m as usize).unwrap()) as i32;
                        if (k & 1) > 0 {
                            c += (k >> 1) + 1;
                        } else {
                            c -= k >> 1;
                        }
                    }
                    decompression_buffer[i] = c as u8;
                    i += 1;
                }
            }

            for y in 0..height {
                let row_skip = y * width;
                if (y & 1) == 1 {
                    for x in 0..width {
                        let index = x + row_skip;
                        let decompression_index = ((width - 1) - x ) + row_skip;
                        pixel_buffer[index][channel] = decompression_buffer[decompression_index];
                    }
                } else {
                    for x in 0..width {
                        let index = x + row_skip;
                        pixel_buffer[index][channel] = decompression_buffer[index];
                    }
                }
            }
        }

        for y in 0..height {
            for x in 0..width {
                bmp_file.change_color_of_pixel(x as u16, y as u16, pixel_buffer[x + (y * width)]).expect("Failed to change color of pixel");
            }
        }

        return bmp_file.contents;
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
                let mut decompressed_size : Option<usize>;

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

            let mut entries_map : HashMap<String, usize> = HashMap::new();
            for (i, entry) in entries.iter().enumerate() {
                entries_map.insert(entry.name.clone(), i);
            }

            return ArchiveIndex{ entries, entries_map, offset : file_offset };
        }
        
        fn parse_ns2_header(_file : &mut FileHelper, _offset : u32) -> ArchiveIndex {
            let entries : Vec<ArchiveEntry> = Vec::new();
            let entries_map : HashMap<String, usize> = HashMap::new();
            return ArchiveIndex{ entries, entries_map, offset : 0 };
        }

        fn parse_header(file : &mut FileHelper, archive_type : &ArchiveType, offset : u32) -> ArchiveIndex {
            match archive_type {
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
            let mut buffer : Vec<u8>;

            if matches!(info.compression, Compression::None) {
                buffer = self.file.read_slice_through_keytable(info.offset, info.size);
            } else if matches!(info.compression, Compression::Spb) {
                buffer = parse_spb_into_bmp(&mut self.file, info.offset, info.size);
            } else if matches!(info.compression, Compression::Lzss) {
                buffer = self.file.read_slice_through_keytable(info.offset, info.size);

                let input = buffer;

                type Lzss = lzss::Lzss<8, 4, 0, { 1 << 8 }, { 2 << 8 }>;
                let writer = lzss::VecWriter::with_capacity(input.len());
                
                buffer = Lzss::decompress_stack(
                    lzss::SliceReader::new(input.as_slice()),
                    writer,
                ).unwrap();
            } else if matches!(info.compression, Compression::Nbz) {
                buffer = self.file.read_slice(info.offset, info.size);

                use bzip2_rs::DecoderReader;
                let input = buffer;

                // First 4 bytes are the original size, the decoder doesn't need this, so we can skip them.
                let mut reader = DecoderReader::new(&input[4..]);
                buffer = Vec::new();
                std::io::copy(&mut reader, &mut buffer).unwrap();
            } else {
                buffer = Vec::new();
            }

            return buffer;
        }
    }
}