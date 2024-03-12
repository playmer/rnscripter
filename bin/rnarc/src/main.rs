use std::{fs::File, io::Write, path::Path};

use clap::Parser;
use nscripter_archive::nscripter_archive::*;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the file to read.
    #[arg(short, long)]
    file: String,

    /// Name of the directory to output files.
    #[arg(short, long)]
    output: String,

    /// Offset of data within archive
    #[arg(long, default_value_t = 0)]
    offset: u32,
}


fn main() {
    let archive_type = ArchiveType::NSA;
    let args = Args::parse();

    let file = std::fs::File::open(args.file).unwrap();
    //let file_bytes = std::fs::read(args.file).unwrap();

    let output_dir = Path::new(&args.output);

    println!("rnarc");
    let mut reader : ArchiveReader = ArchiveReader::new(file, archive_type, args.offset, ArchiveReader::default_keytable());

    for i in 0..reader.index.entries.len() {
        let info = reader.index.entries[i].info();
        let data = reader.extract(info);

        let entry = &reader.index.entries[i];
        let mut entry_name = entry.name.clone();
        
        if matches!(entry.compression, Compression::Nbz) {
            entry_name = entry_name + ".zip";
        }

        let entry_path = Path::new(&entry_name);
        let mut new_path = output_dir.join(entry_path);


        if data.len() == 0 {
            println!("Couldn't extract {} of compression type {:?}", &entry.name, &entry.compression);
            continue;
        } else {
            println!("Extracting: {} to {}, Compression Type {:?}, Size {}", &entry.name, new_path.to_str().unwrap(), &entry.compression, &entry.size);
        }

        std::fs::create_dir_all(&new_path.parent().unwrap()).unwrap();

        let mut file = File::create(&new_path).unwrap();
        file.write_all(&data).unwrap();
    }
}