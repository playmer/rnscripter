#[allow(dead_code)]
use core::panic;
use std::{collections::HashMap, fs::File, io::{ErrorKind, Read, Seek, SeekFrom, Write}, path::Path};

use crate::image::decode_spb;

pub struct FileHelper {
    pub file : File,
    pub key_table : [u8; 256],
    pub position : usize
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

        self.position += N;

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
    
    fn read_u32_le(&mut self) -> u32 {
        const SIZE : usize = std::mem::size_of::<u32>();
        let buffer = self.read_buffer::<SIZE>();
        u32::from_le_bytes(buffer)
    }
    
    fn write_u8_be(&mut self, value : u8) {
        self.file.write_all(&value.to_be_bytes()).unwrap();
    }
    
    fn write_u16_be(&mut self, value : u16) {
        self.file.write_all(&value.to_be_bytes()).unwrap();
    }
    
    fn write_u32_be(&mut self, value : u32) {
        self.file.write_all(&value.to_be_bytes()).unwrap();
    }

    fn write_u32_le(&mut self, value : u32) {
        self.file.write_all(&value.to_le_bytes()).unwrap();
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

    fn write_shiftjis(&mut self, value : &str) {
        use encoding_rs::SHIFT_JIS;
        let (res, _enc, errors) = SHIFT_JIS.encode(&value);
        if errors {
            panic!("Couldn't read a string from this file.");
        }
        
        self.file.write_all(res.as_ref()).unwrap();
    }

    fn read_quoted_shiftjis(&mut self) -> String {
        let mut buffer : Vec<u8> = Vec::new();

        let first_byte = self.read_u8();
        if first_byte != b'\"' {
            panic!("Archive unexpectedly doesn't have a quoted string: {first_byte}.");
        }
            
        loop {
            let byte = self.read_u8();
            
            if byte == b'\"' {
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
    

    fn write_quoted_shiftjis(&mut self, value : &str) {
        use encoding_rs::SHIFT_JIS;
        let (res, _enc, errors) = SHIFT_JIS.encode(&value);
        if errors {
            panic!("Couldn't read a string from this file.");
        }
        
        self.file.write(b"\"").unwrap();
        self.file.write_all(res.as_ref()).unwrap();
        self.file.write(b"\"").unwrap();
    }

    fn seek(&mut self, seek : SeekFrom) {
        self.position = self.file.seek(seek).unwrap() as usize;
    }

    fn read_slice(&mut self, offset : usize, size : usize) -> Vec<u8> {
        self.file.seek(SeekFrom::Start(offset as u64)).unwrap();
        let mut buffer : Vec<u8> = vec![0; size];
        self.file.read(&mut buffer).unwrap();

        self.position += size;

        buffer
    }

    fn read_slice_through_keytable(&mut self, offset : usize, size : usize) -> Vec<u8> {
        let mut output = self.read_slice(offset, size);
        for byte in &mut output {
            *byte = self.key_table[*byte as usize];
        }

        // read_slize alters self.position, don't need to do it redundantly here.

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
    _decompressed_size : Option<usize>,
    pub compression : Compression
}

impl ArchiveEntry {
    pub fn info(&self) -> ArchiveEntryInfo {
        ArchiveEntryInfo {
            offset : self.offset, 
            size : self.size, 
            _decompressed_size : self.decompressed_size, 
            compression : self.compression, 
        }
    }
}

pub struct ArchiveIndex {
    pub entries : Vec<ArchiveEntry>,
    pub entries_map : HashMap<String, usize>,
    pub offset : usize
}

pub struct Archive {
    file : FileHelper,
    pub index : ArchiveIndex,
    pub archive_type : ArchiveType,
}

pub fn extract_bz2(file: File, key_table : [u8; 256]) -> Vec<u8> {
    let mut file = file;
    let size = file.seek(SeekFrom::End(0)).unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut file_helper = FileHelper {file, key_table, position : 0};
    let buffer = file_helper.read_slice(0, size as usize);

    use bzip2_rs::DecoderReader;
    let input = buffer;

    // First 4 bytes are the original size, the decoder doesn't need this, so we can skip them.
    let mut reader = DecoderReader::new(&input[4..]);
    let mut buffer = Vec::new();
    std::io::copy(&mut reader, &mut buffer).unwrap();

    return buffer;
}

impl Archive {
    /*
    fn write_sar_header(&self, archive : &mut Archive) {
        let mut offsets : Vec<(usize, usize)> = Vec::new();
        archive.file.seek(SeekFrom::Start(archive.index.offset as u64));

        // TODO: Should check this cast
        archive.file.write_u16_be(archive.index.entries.len() as u16);

        // Skip writing out the internal offset for now.
        archive.file.seek(SeekFrom::Current(4));

        for entry in &archive.index.entries {
            archive.file.write_shiftjis(&entry.name);
            
            // Skip writing out the internal offset for now.
            offsets.push((archive.file.position, entry.size));
            archive.file.seek(SeekFrom::Current(4));

            // TODO: Should check this cast
            archive.file.write_u32_be(entry.size as u32);
        }

        let file_offset = archive.file.position;
        archive.file.seek(SeekFrom::Start(archive.index.offset as u64 + 2));
        archive.file.write_u32_be(file_offset);

        for (offset_position, entry_size) in offsets {
            archive.file.seek(SeekFrom::Start(archive.index.offset as u64 + 2));
            archive.file.write_u32_be(file_offset);
        }

        //archive.file.write_u32_be(archive.index.offset )
    } 
    */


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
    
    fn parse_ns2_header(file : &mut FileHelper, offset : u32) -> ArchiveIndex {
        let mut entries : Vec<ArchiveEntry> = Vec::new();
        let offset_of_file_data = (file.read_u32_le() + offset) as usize; // Entries start at this address in the file
        let mut file_offset = offset_of_file_data;

        while file.position < (offset_of_file_data - 1) {
            let name = file.read_quoted_shiftjis();
            let size = file.read_u32_le() as usize;
            //let decompressed_size = 0;
            
            let lowercase_name = name.to_lowercase();
            let compression =  if lowercase_name.ends_with(".nbz") {
                Compression::Bzip2
            } else if lowercase_name.ends_with(".spb") {
                Compression::Spb
            } else {
                Compression::None
            };
            

            println!("{name}: {size}: {file_offset}");
            
            entries.push(ArchiveEntry {
                name, offset: file_offset, size, decompressed_size: None, compression
            });

            file_offset += size
        }
        
        let unknown_value = file.read_u8();
        println!("Header end byte: {unknown_value}");
        
        let mut entries_map : HashMap<String, usize> = HashMap::new();
        for (i, entry) in entries.iter().enumerate() {
            entries_map.insert(entry.name.clone(), i);
        }

        ArchiveIndex{ entries, entries_map, offset : 0 }
    }

    fn parse_header(file : &mut FileHelper, archive_type : &ArchiveType, offset : u32) -> ArchiveIndex {
        match archive_type {
            ArchiveType::SAR => Self::parse_sar_header(file, offset),
            ArchiveType::NSA => Self::parse_nsa_header(file, offset),
            ArchiveType::NS2 => Self::parse_ns2_header(file, offset)
        }
    }

    pub fn new(file : std::fs::File, archive_type : ArchiveType, offset : u32, key_table : [u8; 256]) -> Archive {
        let mut file_helper = FileHelper {file, key_table, position : 0};
        let index = Self::parse_header(&mut file_helper, &archive_type, offset);

        Archive {
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
            buffer = decode_spb(self.file.read_slice(info.offset, info.size)).unwrap();
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