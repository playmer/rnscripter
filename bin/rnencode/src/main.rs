use std::path::PathBuf;
use std::{fs::File, io::Write, path::Path};

use clap::Parser;
use nscripter_formats::archive::*;
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

    /// Offset of data within archive, defaults to 0.
    #[arg(long, default_value_t = 0)]
    offset: u32,

    /// Path to a key table, this adds a layer of obfuscation to created archives. A key table should be a file containing 256 bytes, which
    /// each have a unique number [0, 255]. Effectively this is a lookup table for whenever we would write a byte, we lookup in the key table
    /// what we should write instead. You may pass an executable file which will be scanned looking for an embedded key table.
    /// 
    /// When playing a game with obfuscated archives in ONScripter, you would pass this key table file with the 
    /// `--key-exe` command line option so it can do the same in reverse.
    #[arg(short, long)]
    key_file: Option<String>,
    
    /// When creating nsa archives, this flag decides if we should use BZip2 compression on BMP and WAV files. 
    /// 
    /// Note: Equivalent of the enhanced flag in nsamake from ONScripter.
    #[arg(short, long, default_value_t = false)]
    bzip2: bool,

    /// When creating nsa archives, this flag decides if we should use SPB compression on BMP files.
    /// 
    /// Note: If used in conjunction with bzip2 flag, we will only compress BMP with SPB compression, only WAV files will use BZip2.
    /// There haven't been tests done to determine which is, on average, better for BMP files, so ultimately you should test and check
    /// on your own data.
    #[arg(short, long, default_value_t = false)]
    spb: bool,
    
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

fn collect_entries(archive_dir: &Path, output_file: &Path) -> Vec<PathBuf>
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
    }

    return entries_to_archive;
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
    
    let lowercase_name = arguments.output.to_lowercase();
    if lowercase_name.ends_with(".sar") || lowercase_name.ends_with(".nsa") || lowercase_name.ends_with(".ns2") {
        let file = File::create(&output).unwrap();

        //std::fs::create_dir(&output).unwrap();
        let entries_to_archive = collect_entries(&path, &output);

        if lowercase_name.ends_with(".sar") {
            Archive::create_sar_archive(file, &path, entries_to_archive, nscripter_formats::default_keytable());
        }
        else if lowercase_name.ends_with(".nsa") {
            Archive::create_nsa_archive(file, &path, entries_to_archive, arguments.offset as usize, nscripter_formats::default_keytable(), arguments.bzip2, arguments.spb);
        }
        else if lowercase_name.ends_with(".ns2")  {
        }
    }
    else {

    }
}
