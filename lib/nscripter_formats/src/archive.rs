use core::panic;
use std::{collections::HashMap, fs::File, io::{ErrorKind, Read, Seek, SeekFrom, Write}, path::{Path, PathBuf}};
use bzip2::read::{BzDecoder, BzEncoder};

use crate::image::{decode_spb, encode_spb};

pub struct FileHelper {
    pub file : File,
    pub key_table : [u8; 256],
    pub position : usize
}

#[allow(dead_code)]
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
    
    fn write_buffer(&mut self, buffer: &[u8]) {
        self.file.write_all(buffer).unwrap();
        self.position += buffer.len();
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
    
    fn write_u8(&mut self, value : u8) {
        self.write_buffer(&value.to_le_bytes());
    }
    
    fn write_u16_be(&mut self, value : u16) {
        self.write_buffer(&value.to_be_bytes());
    }
    
    fn write_u32_be(&mut self, value : u32) {
        self.write_buffer(&value.to_be_bytes());
    }

    fn write_u32_le(&mut self, value : u32) {
        self.write_buffer(&value.to_le_bytes());
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

        self.write_buffer(res.as_ref());
        self.write_buffer(b"\0");
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
        
        self.write_buffer(b"\"");
        self.write_buffer(res.as_ref());
        self.write_buffer(b"\"");
    }

    fn write_file(&mut self, src: &mut dyn Read, buffer: &mut [u8; 64536])
    {
        loop {
            match src.read(buffer) {
                Ok(size) => {
                    if size == 0 {
                        return;
                    }

                    self.write_buffer(&buffer[0..size]);
                },
                Err(err) => {
                    panic!("Error reading file: {}", err);
                }
            }
        }
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

        // read_slice alters self.position, don't need to do it redundantly here.

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

    use bzip2::read::{BzDecoder};
    let input = buffer;

    // First 4 bytes are the original size, the decoder doesn't need this, so we can skip them.
    let mut reader = BzDecoder::new(&input[4..]);
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
    
    pub fn create_sar_archive(file: File, root_dir: &Path, entries : Vec<PathBuf>, key_table : [u8; 256]) -> bool {
        let mut file_helper = FileHelper {file, key_table, position : 0};

        if (u16::MAX as usize) < entries.len() {
            return false;
        }

        let mut entry_offset_locations = Vec::new();


        file_helper.write_u16_be(entries.len() as u16);
        file_helper.write_u32_be(0);

        for entry in &entries {
            let fullpath = root_dir.join(entry);
            let mut entry_file = std::fs::File::open(&fullpath).unwrap();
            let entry_size = entry_file.seek(SeekFrom::End(0)).unwrap();
            let entry_inner_path = entry.to_str().unwrap();

            file_helper.write_shiftjis(&entry_inner_path);

            // Note down where this offset value is for later.
            entry_offset_locations.push(file_helper.position);            
            file_helper.write_u32_be(0);
            file_helper.write_u32_be(entry_size as u32);
        }

        let end_of_header = file_helper.position;

        file_helper.seek(SeekFrom::Start(2));
        file_helper.write_u32_be(end_of_header as u32);
        file_helper.seek(SeekFrom::Start(end_of_header as u64));
        
        // We only want to init this once for all files, so the buffer lives outside of the read_file_into_file call.
        let mut buffer : [u8; 64536] = [0; 64536];
        
        for (entry_file_name, entry_offset_location) in entries.iter().zip(&entry_offset_locations) {
            let fullpath = root_dir.join(&entry_file_name);
            let mut entry_file = std::fs::File::open(&fullpath).unwrap();
            let entry_offset = file_helper.position;

            file_helper.seek(SeekFrom::Start(*entry_offset_location as u64));
            file_helper.write_u32_be((entry_offset - end_of_header) as u32);

            file_helper.seek(SeekFrom::Start(entry_offset as u64));
            file_helper.write_file(&mut entry_file, &mut buffer);
        }
        
        return true;
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

    
    fn file_encoding_to_use(file_path: &Path, bzip2: bool, spb: bool) -> Compression {
        if !bzip2 && !spb {
            return Compression::None;
        }

        let entry_inner_path = file_path.to_str().unwrap();
        let lowercase_name = entry_inner_path.to_lowercase();
        if !lowercase_name.ends_with(".wav") && !lowercase_name.ends_with(".bmp") {
            return Compression::None;
        }

        let mut entry_file = std::fs::File::open(&file_path).unwrap();
        let mut header_bytes : [u8; 4] = [0; 4];
        entry_file.read(&mut header_bytes);

        if header_bytes[0] == b'R' && header_bytes[1] == b'I' && header_bytes[2] == b'F' && header_bytes[3] == b'F' {
            match bzip2 {
                true => return Compression::Bzip2,
                false => return Compression::None
            }
        } else if header_bytes[0] == b'B' && header_bytes[1] == b'M' {
            if spb {
                return Compression::Spb;
            }
            else if bzip2 {
                return Compression::Bzip2;
            }
            else {
                return Compression::None;
            }
        } else {
            return Compression::None
        }
    }

    fn compression_to_byte(compression: Compression) -> u8 {
        match compression {
            Compression::None => return 0,
            Compression::Spb => return 1,
            Compression::Lzss => return 2,
            Compression::Bzip2 => return 4,
        }
    }
    
    pub fn create_nsa_archive(file: File, root_dir: &Path, entries : Vec<PathBuf>, offset: usize, key_table : [u8; 256], bzip2: bool, spb: bool) -> bool {
        let mut file_helper = FileHelper {file, key_table, position : 0};

        if (u16::MAX as usize) < entries.len() {
            return false;
        }

        let mut entry_offset_locations: Vec<(usize, Compression)> = Vec::new();

        file_helper.write_u16_be(entries.len() as u16);
        file_helper.write_u32_be(0);

        for entry in &entries {
            let full_path = root_dir.join(&entry);
            let entry_inner_path = entry.to_str().unwrap();

            file_helper.write_shiftjis(&entry_inner_path);

            let encoding = Self::file_encoding_to_use(&full_path, bzip2, spb);
            let compress_byte = Self::compression_to_byte(encoding);
            file_helper.write_u8(compress_byte);

            // Note down where this offset value is for later.
            entry_offset_locations.push((file_helper.position, encoding));
            file_helper.write_u32_be(0); // offset
            file_helper.write_u32_be(0); // size
            file_helper.write_u32_be(0); // decompressed size
        }

        let end_of_header = file_helper.position + offset;

        file_helper.seek(SeekFrom::Start(2));
        file_helper.write_u32_be(end_of_header as u32);
        file_helper.seek(SeekFrom::Start(end_of_header as u64));
        
        // We only want to init this once for all files, so the buffer lives outside of the read_file_into_file call.
        let mut buffer : [u8; 64536] = [0; 64536];
        
        for (entry_file_name, (entry_offset_location, compression)) in entries.iter().zip(&entry_offset_locations) {
            let fullpath = root_dir.join(&entry_file_name);
            let mut entry_file = std::fs::File::open(&fullpath).unwrap();
            let entry_offset = file_helper.position;
            
            file_helper.seek(SeekFrom::Start(entry_offset as u64));

            
            let decompressed_size = entry_file.seek(SeekFrom::End(0)).unwrap();
            entry_file.seek(SeekFrom::Start(0)).unwrap();

            let compressed_size = match compression {
                Compression::None => {
                    file_helper.write_file(&mut entry_file, &mut buffer);
                    decompressed_size
                },
                Compression::Spb => {
                    //encode_spb()
                    decompressed_size
                },
                Compression::Lzss => {
                    decompressed_size
                },
                Compression::Bzip2 => {
                    let mut compressor = BzEncoder::new(entry_file, bzip2::Compression::best());
                    file_helper.write_file(&mut compressor, &mut buffer);
                    decompressed_size
                },
            };

            file_helper.seek(SeekFrom::Start(*entry_offset_location as u64));
            file_helper.write_u32_be((entry_offset - end_of_header) as u32);
            file_helper.write_u32_be(compressed_size as u32);
            file_helper.write_u32_be(decompressed_size as u32);
        }
        
        return true;
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

    pub fn open_file(file : std::fs::File, archive_type : ArchiveType, offset : u32, key_table : [u8; 256]) -> Archive {
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
            let input = buffer;

            // First 4 bytes are the original size, the decoder doesn't need this, so we can skip them.
            let mut reader = BzDecoder::new(&input[4..]);
            buffer = Vec::new();
            std::io::copy(&mut reader, &mut buffer).unwrap();
        } else {
            buffer = Vec::new();
        }

        buffer
    }
}