use std::path::PathBuf;
use std::{fs::File, io::Write, path::Path};

use clap::Parser;
use nscripter_formats::archive::*;
use nscripter_formats::image::decode_spb;
use walkdir::WalkDir;

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

fn detect_file_type(data: &Vec<u8>) -> String {
    if data[0] == b'R' && data[1] == b'I' && data[2] == b'F' && data[3] == b'F' {
        return ".wav".to_string()
    } else if data[0] == b'B' && data[1] == b'M' {
        return ".bmp".to_string()
    } else {
        return "".to_string()
    }

}

fn archive_directory(archive_dir: &Path, output_file: &Path)
{
    let mut entries_to_archive : Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(&archive_dir) {
        let entry = entry.unwrap();
        let entry_fullpath = entry.path();

        if std::fs::metadata(&entry_fullpath).unwrap().is_dir() {
            continue;
        }

        let entry = entry_fullpath.strip_prefix(&archive_dir).unwrap();

        entries_to_archive.push(entry.to_owned());
        //println!("{}", entry.display());
    }

    let file = File::create(&output_file).unwrap();
    Archive::create_sar_archive(file, archive_dir, entries_to_archive, 0, nscripter_formats::default_keytable());
}

fn main() {
    let arguments = Arguments::parse();
    let output = Path::new(&arguments.output);
    let path = Path::new(&arguments.path);

    if output.exists() {
        if !arguments.force {
            println!("{} exists, if you wish to delete it's contents and write out the archive from scratch, pass --force or -f.", arguments.output);
            return;
        } else if output.is_dir() {
            std::fs::remove_dir_all(&output).unwrap();
        } else {
            std::fs::remove_file(&output).unwrap();
        }
    }
    
    //std::fs::create_dir(&output).unwrap();

    if path.is_dir() {
        archive_directory(&path, &output);
    } else {
    }
}
