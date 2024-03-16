#[allow(dead_code)]
use core::panic;
use std::{collections::HashMap, fs::File, io::{ErrorKind, Read, SeekFrom}};

pub struct FileHelper {
    pub file : File,
    pub key_table : [u8; 256]
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

        buffer
    }

    fn read_u8(&mut self) -> u8 {
        const SIZE : usize = std::mem::size_of::<u8>();
        let buffer = self.read_buffer::<SIZE>();
        u8::from_be_bytes(buffer)
    }

    fn read_u16_be(&mut self) -> u16 {
        const SIZE : usize = std::mem::size_of::<u16>();
        let buffer = self.read_buffer::<SIZE>();
        u16::from_be_bytes(buffer)
    }

    fn read_u32_be(&mut self) -> u32 {
        const SIZE : usize = std::mem::size_of::<u32>();
        let buffer = self.read_buffer::<SIZE>();
        u32::from_be_bytes(buffer)
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

        res.to_string()
    }

    fn seek(&mut self, seek : SeekFrom) {
        use std::io::Seek;
        self.file.seek(seek).unwrap();
    }

    fn read_slice(&mut self, offset : usize, size : usize) -> Vec<u8> {
        use std::io::Seek;

        self.file.seek(SeekFrom::Start(offset as u64)).unwrap();
        let mut buffer : Vec<u8> = vec![0; size];
        self.file.read(&mut buffer).unwrap();

        buffer
    }

    fn read_slice_through_keytable(&mut self, offset : usize, size : usize) -> Vec<u8> {
        let mut output = self.read_slice(offset, size);
        for byte in &mut output {
            *byte = self.key_table[*byte as usize];
        }

        output
    }
}

#[derive(Clone, Copy, Debug)] 
pub enum Compression {
    None = 0,
    Spb = 1,
    Lzss = 2, // Lempel–Ziv–Storer–Szymanski Compression
    Bzip2 = 4, // Bzip2 Compression: sometimes embedded files have "nbz" extension, these are just Bzip2.
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
        ArchiveEntryInfo {
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
pub fn parse_spb_into_bmp(file : &mut FileHelper, offset : usize, size : usize) -> Vec<u8> {
    //file.seek(SeekFrom::Start(offset as u64));
    //let width = file.read_u16_be() as usize;
    //let height = file.read_u16_be() as usize;

    let buffer = file.read_slice_through_keytable(offset, size);
    
    use bitbuffer::{BitReadBuffer, BitReadStream, BigEndian};
    let buffer = BitReadBuffer::new(&buffer, BigEndian);
    let mut bitstream = BitReadStream::new(buffer);

    let width = bitstream.read_int::<u16>(16).unwrap() as usize;
    let height = bitstream.read_int::<u16>(16).unwrap() as usize;
    
    let mut pixel_buffer : Vec<u8> = vec![0; (width * height + 4) * 3];

    // Read each channel of image data, in BGR order.
    for channel in (0..3).rev() {
        let start = (width * height + 4) * channel;
        let end = (width * height + 4) * (channel + 1);
        let channel_buffer = &mut pixel_buffer[start..end];
        channel_buffer[0] = bitstream.read_int::<u8>(8).unwrap();
        let mut i : usize = 1;

        while i < (width * height) {
            let mut data_byte = channel_buffer[i - 1];

            // Read a 3 bit header from the stream, 3 bits means range is [0,7]
            // This header helps determine how we stamp the next 4 bytes.
            let header = bitstream.read_int::<u8>(3).unwrap();

            let bits_to_read : u8 = match header {
                // Stamp 4 identical bytes
                0 => {
                    channel_buffer[i]     = data_byte;
                    channel_buffer[i + 1] = data_byte;
                    channel_buffer[i + 2] = data_byte;
                    channel_buffer[i + 3] = data_byte;
                    i += 4;
                    continue;
                }
                6 => {
                    channel_buffer[i]     = bitstream.read_int::<u8>(8).unwrap();
                    channel_buffer[i + 1] = bitstream.read_int::<u8>(8).unwrap();
                    channel_buffer[i + 2] = bitstream.read_int::<u8>(8).unwrap();
                    channel_buffer[i + 3] = bitstream.read_int::<u8>(8).unwrap();
                    i += 4;
                    continue;
                }
                // bits_to_read is within  [3,7]
                1..=5 => {
                    header + 2
                }
                // bits_to_read is within [1,2], depending on a 1 bit read.
                // escape sequence in case of adding one or subtracting zero.
                7 => {
                    bitstream.read_int::<u8>(1).unwrap() + 1
                }
                _ => {
                    panic!("Impossible value for n (a 3 bit integer) when decoding SPB: {header}");
                }
            };
            
            // bits_to_read is within [1,7]
            for _ in 0..4 {
                let modify_byte = bitstream.read_int::<u8>(bits_to_read as usize).unwrap();
                
                // The last bit read is used to determine how we'll be modifying the data byte, after
                // determining that we throw away that bit.
                let add = (modify_byte & 1) > 0;
                let modify_byte = modify_byte >> 1;
                
                if add {
                    data_byte += modify_byte + 1;
                } else {
                    data_byte -= modify_byte;
                }

                channel_buffer[i] = data_byte;
                i += 1;
            }
        }
    }

    let r_buffer = &pixel_buffer[0..(width * height + 4)];
    let g_buffer = &pixel_buffer[(width * height + 4)..(width * height + 4) * 2];
    let b_buffer = &pixel_buffer[(width * height + 4) * 2..(width * height + 4) * 3];

    // We've read all the channels, we can comfortably blit out a BMP now.
    let mut bmp_file = bmp_rust::bmp::BMP::new(height as i32, width as u32, None);
    for y in 0..height {
        let row_skip = y * width;
        for x in 0..width {
            // If we're on an odd row, we read backwards
            let i = if (y & 1) == 1 {
                ((width - 1) - x ) + row_skip
            } else {
                x + row_skip
            };

            let r = r_buffer[i];
            let g = g_buffer[i];
            let b = b_buffer[i];
            bmp_file.change_color_of_pixel(x as u16, y as u16, [r,g,b,255]).expect("Failed to change color of pixel");
        }
    }

    bmp_file.contents
}

impl ArchiveReader {
    fn parse_sar_header(file : &mut FileHelper, offset : u32) -> ArchiveIndex {
        let mut entries : Vec<ArchiveEntry> = Vec::new();
        let num_of_entries = file.read_u16_be();
        let file_offset = (file.read_u32_be() + offset) as usize; // Entries start at this address in the file

        for _ in 0..num_of_entries {
            let name = file.read_shiftjis();
            let compression = Compression::None;
            let offset = file.read_u32_be() as usize + file_offset;
            let size = file.read_u32_be() as usize;
            let decompressed_size : Option<usize> = Some(size);

            entries.push(ArchiveEntry {
                name, offset, size, decompressed_size, compression
            });
        }

        let mut entries_map : HashMap<String, usize> = HashMap::new();
        for (i, entry) in entries.iter().enumerate() {
            entries_map.insert(entry.name.clone(), i);
        }

        ArchiveIndex{ entries, entries_map, offset : file_offset }
    }

    fn parse_nsa_header(file : &mut FileHelper, offset : u32) -> ArchiveIndex {
        let mut entries : Vec<ArchiveEntry> = Vec::new();
        let num_of_entries = file.read_u16_be();
        let file_offset = (file.read_u32_be() + offset) as usize; // Entries start at this address in the file

        println!("Number of entries: {num_of_entries}; File Offset {file_offset}");

        for _ in 0..num_of_entries {
            let name = file.read_shiftjis();

            let compression = match file.read_u8() {
                0 => {
                    let lowercase_name = name.to_lowercase();
                    if lowercase_name.ends_with(".nbz") {
                        Compression::Bzip2
                    } else if lowercase_name.ends_with(".spb") {
                        Compression::Spb
                    } else {
                        Compression::None
                    }
                },
                1 => Compression::Spb,
                2 => Compression::Lzss,
                4 => Compression::Bzip2,
                _ => panic!("File is using an unknown Compression type.")
            };

            let offset = file.read_u32_be() as usize + file_offset;
            let size = file.read_u32_be() as usize;
            let mut decompressed_size : Option<usize> = Some(file.read_u32_be() as usize);

            // ONScripter notes decompression of these just for the sake of filling this value as a
            // large potential slowdown depending on the archive. We'll follow their lead in ignoring
            // it until the entry is actually opened.
            if matches!(compression, Compression::Bzip2) || matches!(compression, Compression::Spb) {
                decompressed_size = None;
            }

            entries.push(ArchiveEntry {
                name, offset, size, decompressed_size, compression
            });
        }

        let mut entries_map : HashMap<String, usize> = HashMap::new();
        for (i, entry) in entries.iter().enumerate() {
            entries_map.insert(entry.name.clone(), i);
        }

        ArchiveIndex{ entries, entries_map, offset : file_offset }
    }
    
    fn parse_ns2_header(_file : &mut FileHelper, _offset : u32) -> ArchiveIndex {
        let entries : Vec<ArchiveEntry> = Vec::new();
        let entries_map : HashMap<String, usize> = HashMap::new();
        ArchiveIndex{ entries, entries_map, offset : 0 }
    }

    fn parse_header(file : &mut FileHelper, archive_type : &ArchiveType, offset : u32) -> ArchiveIndex {
        match archive_type {
            ArchiveType::SAR => Self::parse_sar_header(file, offset),
            ArchiveType::NSA => Self::parse_nsa_header(file, offset),
            ArchiveType::NS2 => Self::parse_ns2_header(file, offset)
        }
    }

    pub fn new(file : std::fs::File, archive_type : ArchiveType, offset : u32, key_table : [u8; 256]) -> ArchiveReader {
        let mut file_helper = FileHelper {file, key_table};
        let index = Self::parse_header(&mut file_helper, &archive_type, offset);

        ArchiveReader {
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
        } else if matches!(info.compression, Compression::Bzip2) {
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

        buffer
    }
}