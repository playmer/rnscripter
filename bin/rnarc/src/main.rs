use std::{fs::File, io::Write, path::Path};

use clap::Parser;
use nscripter_formats::archive::*;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the file to read.
    #[arg(short, long)]
    file: String,

    // If the file is a directory, we'll read all possible files from it.
    #[arg(short, long, default_value_t = false)]
    directory: bool,

    // If we should decompress spb files
    #[arg(short, long, default_value_t = true)]
    spb_decode: bool,

    /// Name of the directory to output files.
    #[arg(short, long)]
    output: String,

    /// Offset of data within archive
    #[arg(long, default_value_t = 0)]
    offset: u32,
}

fn extract_files(file : File, archive_type : ArchiveType, offset : u32, output_dir : &Path) {
    let mut reader : ArchiveReader = ArchiveReader::new(file, archive_type, offset, nscripter_formats::default_keytable());

    for i in 0..reader.index.entries.len() {
        let info = reader.index.entries[i].info();

        if matches!(info.compression, Compression::Spb) {
            println!("Extracting {}", &reader.index.entries[i].name);
        }
        
        let data = reader.extract(info);

        let entry = &reader.index.entries[i];
        let entry_name = entry.name.clone();
        let entry_path = Path::new(&entry_name);
        let new_path = output_dir.join(entry_path);

        std::fs::create_dir_all(&new_path.parent().unwrap()).unwrap();

        let mut file = File::create(&new_path).unwrap();
        file.write_all(&data).unwrap();
    }
}


/*
fn spb_test() {
    {
        let file = bmp_rust::bmp::BMP::new_from_file("games\\netannad_og\\arc_nsa\\version.bmp");
    
        let mut pixel_buffer : Vec<[u8; 3]> = Vec::new();
        let bmp_pixel_data = file.get_pixel_data().unwrap();
        let height = bmp_pixel_data.len() as u16;
        let width = bmp_pixel_data[0].len() as u16;
    
        for row in bmp_pixel_data {
            for pixel in row {
                pixel_buffer.push([pixel[0], pixel[1], pixel[2]]);
            }
        }
    
        let image = nscripter_formats::image::Image{pixel_buffer, width, height};
    
        //nscripter_formats::image::Image
        let file_data = nscripter_formats::image::encode_spb(image);
        
        let mut file = File::create("games\\netannad_og\\arc_nsa\\version.test.spb").unwrap();
        file.write_all(&file_data).unwrap();
    }


    {
        let buffer : Vec<u8> = std::fs::read("games\\netannad_og\\arc_nsa\\version.test.spb").unwrap();
        let bmp_file = nscripter_formats::image::decode_spb(buffer);
        
        let mut file = File::create("games\\netannad_og\\arc_nsa\\version.test.bmp").unwrap();
        file.write_all(&bmp_file).unwrap();

        //let file = FileHelper {
        //    File::open("games\\netannad_og\\arc_nsa\\version.spb").unwrap(), 
        //    nscripter_formats::default_keytable()
        //};
        //parse_spb_into_bmp(, offset, size)
        
    }

    {
        let buffer1 : Vec<u8> = std::fs::read("games\\netannad_og\\arc_nsa\\version.spb").unwrap();
        let buffer2 : Vec<u8> = std::fs::read("games\\netannad_og\\arc_nsa\\version.test.spb").unwrap();
        nscripter_formats::image::compare_spb(buffer1, buffer2);
    }
}
 */

fn main() {
    //spb_test();
    //return;

    let args = Args::parse();
    let output_dir = Path::new(&args.output);

    if args.directory {
        let paths = std::fs::read_dir(args.file).unwrap();

        for path in paths {
            let path = path.unwrap().path();
            let file_name = path.file_name().unwrap().to_str().unwrap().to_lowercase();
            
            // Technically some of these can spread between different archives, and if they're not named sequentially there could be issues,
            // and if they're not just numbers between "arc" and the archive type that's also wrong, but for now lets just assume this is
            // fine.
            let archive_type : ArchiveType = if file_name.starts_with("arc") && file_name.ends_with(".ns2") {
                ArchiveType::NS2
            } else if file_name.starts_with("arc") && file_name.ends_with(".nsa") {
                ArchiveType::NSA
            } else if file_name.starts_with("arc") && file_name.ends_with(".sar") {
                ArchiveType::SAR
            } else {
                println!("{file_name}");
                continue;
            };
            
            let file = std::fs::File::open(&path).unwrap();

            let output_dir = output_dir.join(file_name.replace(".", "_"));
            extract_files(file, archive_type, args.offset, &output_dir);
        }
    } else {
        let file = std::fs::File::open(&args.file).unwrap();
        let file_path = Path::new(&args.file);
        let file_name = file_path.to_str().unwrap().to_lowercase();

        let archive_type : ArchiveType = if file_name.starts_with("arc") && file_name.ends_with(".ns2") {
            ArchiveType::NS2
        } else if file_name.starts_with("arc") && file_name.ends_with(".nsa") {
            ArchiveType::NSA
        } else if file_name.starts_with("arc") && file_name.ends_with(".sar") {
            ArchiveType::SAR
        } else {
            panic!("Can't detect the archive type based on extension name! Should be `.ns2`, `.nsa`, or `.sar`");
        };

        extract_files(file, archive_type, args.offset, &output_dir);
    }
}
