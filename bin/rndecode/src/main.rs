use std::{fs::File, io::Write, path::Path};

use clap::{Args, Parser, Subcommand};
use nscripter_formats::archive::*;
use nscripter_formats::image::decode_spb;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Arguments {
    /// Name of the path to read
    #[arg(short, long)]
    path: String,

    /// Name of the directory to output files.
    #[arg(short, long)]
    output: String,

    /// Offset of data within archive
    #[arg(long, default_value_t = 0)]
    offset: u32,
    
    /// This will determine if we should list out File by File what we're extracting.
    #[arg(short, long, default_value_t = false)]
    verbose: bool,
    
    /// This will ensure we entirely overwrite the output directory. All existing files within will be deleted, not just ones that might be overwritten.
    /// 
    /// Otherwise, we'll fail out when trying to overwrite a file.
    #[arg(short, long, default_value_t = false)]
    force: bool,
}

fn extract_files(path : &Path, archive_type : ArchiveType, offset : u32, output_dir : &Path, verbose: bool) {
    let file = std::fs::File::open(&path).unwrap();
    let mut reader : Archive = Archive::new(file, archive_type, offset, nscripter_formats::default_keytable());

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
        
        if verbose {
            println!("Extracting file {} from archive {} to {}", entry_path.to_str().unwrap(), path.to_str().unwrap(), new_path.to_str().unwrap());
        }

        let mut file = File::create(&new_path).unwrap();
        file.write_all(&data).unwrap();
    }
}


fn detect_file_type(data: &Vec<u8>) -> String {
    if data[0] == b'R' && data[1] == b'I' && data[2] == b'F' && data[3] == b'F' {
        return ".wav".to_string()
    } else if data[0] == b'B' && data[1] == b'M' {
        return ".bmp".to_string()
    } else {
        return "".to_string()
    }

}

fn process_file(path: &Path, arguments : &Arguments) {
    let file_name = path.file_name().unwrap().to_str().unwrap().to_lowercase();
    let output_dir = Path::new(&arguments.output);
    
    // Technically some of these can spread between different archives, and if they're not named sequentially there could be issues,
    // and if they're not just numbers between "arc" and the archive type that's also wrong, but for now lets just assume this is
    // fine.
    let archive_type : ArchiveType = if file_name.starts_with("arc") && file_name.ends_with(".ns2") {
        ArchiveType::NS2
    } else if file_name.starts_with("arc") && file_name.ends_with(".nsa") {
        ArchiveType::NSA
    } else if file_name.starts_with("arc") && file_name.ends_with(".sar") {
        ArchiveType::SAR
    } else if file_name.ends_with(".nbz") {
        let file = std::fs::File::open(&path).unwrap();
        let decoded_data = extract_bz2(file, nscripter_formats::default_keytable());
        let file_ext = detect_file_type(&decoded_data);
        
        let new_path = output_dir.join(format!("{}{}", path.file_stem().to_owned().unwrap().to_str().unwrap(), file_ext));
        let mut file = File::create(&new_path).unwrap();                
        file.write_all(&decoded_data).unwrap();
        
        if arguments.verbose {
            println!("Decoding loose nbz file {} to {}", path.to_str().unwrap(), new_path.to_str().unwrap());
        }
        return;
    } else if file_name.ends_with(".spb") {
        let data = std::fs::read(&path).unwrap();
        let decoded_data = decode_spb(data).unwrap();
        
        let new_path = output_dir.join(path.file_stem().to_owned().unwrap().to_str().unwrap());
        let mut file = File::create(&new_path).unwrap();                
        file.write_all(&decoded_data).unwrap();

        if arguments.verbose {
            println!("Decoding loose spb image {} to {}", path.to_str().unwrap(), new_path.to_str().unwrap());
        }
        return;
    }
    else {
        let new_path = output_dir.join(path.file_name().to_owned().unwrap().to_str().unwrap());
        
        if arguments.verbose {
            println!("Copying loose file {} to {}", path.to_str().unwrap(), new_path.to_str().unwrap());
        }
        std::fs::copy(&path, new_path).unwrap();
        return;
    };
    
    let output_dir = output_dir.join(file_name);
    extract_files(&path, archive_type, arguments.offset, &output_dir, arguments.verbose);
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
    let arguments = Arguments::parse();
    let output_dir = Path::new(&arguments.output);
    let path = Path::new(&arguments.path);

    if output_dir.exists() {
        if !arguments.force {
            println!("{} exists, if you wish to delete it's contents and write out the archive from scratch, pass --force or -f.", arguments.output);
            return;
        } else {
            std::fs::remove_dir_all(output_dir).unwrap();
        }
    }
    
    std::fs::create_dir(output_dir).unwrap();

    if path.is_dir() {
        let paths = std::fs::read_dir(path).unwrap();

        for path in paths {
            let path = path.unwrap().path();
            process_file(&path, &arguments);
        }
    } else {
        process_file(&path, &arguments);
    }
}
