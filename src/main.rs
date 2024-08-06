extern crate simplelog;
extern crate chrono;

use simplelog::*;
use log::{info, error};
use std::{fs::{self, File, OpenOptions, remove_file}, path::{Path, PathBuf}, time::{Instant, Duration, SystemTime}, io::{Write, ErrorKind}};
use chrono::*;
use time::UtcOffset;
use serde::Deserialize;
use walkdir::WalkDir;
use toml;
use zip::write::{FileOptions, ZipWriter};
use std::collections::HashMap;
use filetime::FileTime;
use std::process;

const APP_NAME: &str = "LogRC";

#[derive(Deserialize)]
struct Directory {
    path: String,
    filenamecontains: String,
    retentionindays: u64,
    compress: bool,
    movetopath: String
}

#[derive(Deserialize)]
struct Directories {
    directory: Vec<Directory>,
}

#[derive(Deserialize)]
struct Application {
    logretentionindays: u64,
}

#[derive(Deserialize)]
struct ConfigFile {
    directories: Directories,
    application: Application,
}

fn load_config(path: &str) -> Result<ConfigFile, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(path)?;
    let config_file: ConfigFile = toml::from_str(&contents)?;
    Ok(config_file)
}

fn starttask(days: &u64) -> Instant{

    // Capture the start time
    let start_time = Instant::now();

    //error!("Bright red error");
    //warn!("This is a warning.");
    //debug!("Example debug");
    //info!("This only appears in the log file");

    // Initialize the logger
    init_logger(APP_NAME).expect("Failed to initialize logger");
    
    // Start up text
    info!("Starting up...");

    // Verify Application config settings
    if config_application_setting_checker(&days) {
        
        // Remove old Application log files
        info!("Application log retention: {} days", days);
        remove_old_files("log", APP_NAME, days).expect("Failed to remove application logs past retention");
        
        } else {
        // Should work on making the exit call get back to main
        process::exit(1);

    }

    start_time

}

fn endtasks(start_time: Instant){

    // Calculate the elapsed time
    let as_sec: u64 = start_time.elapsed().as_secs();
    
    // Print the elapsed time
    info!("Application ran for: {} second(s)", as_sec);
    
}

fn setuplogfilename (log_name: &str) -> PathBuf{

    // Get the current date
    let current_date = Local::now().date_naive();

    // Convert the date to a string
    let date_string = current_date.format("%Y-%m-%d").to_string();

    // Create the directory if it doesn't exist
    std::fs::create_dir_all("log").expect("Failed to create log directory");

    // Get a log name that does not exist
    let log_directory: PathBuf = Path::new("log").to_path_buf();
    let log_file_new_path: PathBuf = get_new_log_path(&date_string, log_directory, &log_name);

    // returns the path
    log_file_new_path
}

fn get_new_log_path (date: &str, basepath: PathBuf, log_name: &str) -> PathBuf{

    let mut log_int = 1;
    let mut found: bool = false;
    let mut log_file_path = basepath.clone();

    while !found {
        let log_file_name = format!("{}_{}-{}.log", date, log_name, log_int);
        log_file_path = basepath.join(&log_file_name);

        if Path::new(&log_file_path).exists() {
            log_int += 1;
        } else {
            found = true;
        }
    }

    return log_file_path

}

fn init_logger(log_name: &str) -> Result<(), Box<dyn std::error::Error>> {

    // Get the log file path
    let log_file_path = setuplogfilename(&log_name);

    // Open the log file
    let log_file = File::create(log_file_path)?;

    // Get the UTC offset for the log datetime
    let local_offset = UtcOffset::current_local_offset().expect("Failed to get local UTC offset");

    // Custom log format to include the function name
    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .set_time_offset(local_offset)
        .set_location_level(LevelFilter::Debug)  // Remove this line
        .build();

    // Initialize the logger
    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Debug, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
            WriteLogger::new(LevelFilter::Debug, config, log_file),
        ]
    ).unwrap();
    
    Ok(())
}

fn remove_old_files(dir_path: &str, search_str: &str, days: &u64) -> std::io::Result<()> {
    let now = SystemTime::now();
    let max_age = Duration::from_secs((*days * 24 * 60 * 60).into());
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
                if file_name.contains(search_str){
                    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
                        if extension == "log" || extension == "txt" || extension == "zip" {
                            if let Ok(metadata) = fs::metadata(&path) {
                                if let Ok(modified_time) = metadata.modified() {
                                    if let Ok(duration) = now.duration_since(modified_time) {
                                        if duration > max_age {
                                            if let Err(e) = fs::remove_file(&path) {
                                                error!("Error removing file {}: {}", path.display(), e);
                                            } else {
                                                info!("Removed file: '{}'", path.display());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn group_and_compress_files(dir_path: &str, search_string: &str) -> std::io::Result<()> {
    let mut file_groups: HashMap<String, (PathBuf, Vec<PathBuf>, DateTime<FixedOffset>)> = HashMap::new();
    let today = Local::now().date_naive();

    // Walk through the directory
    for entry in WalkDir::new(dir_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && path.file_name().unwrap().to_str().unwrap().contains(search_string) {

            // Get file creation time
            let metadata = fs::metadata(path)?;
            let created: DateTime<Utc> = metadata.created()?.into();

             // Convert to local time with offset
             let local_time = created.with_timezone(&Local);
             let offset = local_time.offset().fix();
             let created_with_offset = created.with_timezone(&offset);
             let file_date = created_with_offset.date_naive();

            // Skip if not a txt or log file
            if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
                if extension != "log" && extension != "txt" {
                    continue;
                }
            }

            // Skip files created today
            if file_date == today {
                info!("Not compressing file: {:?} because it was made today", path.file_name().unwrap());
                continue;
            }

            // Use only date for grouping
            let date_str = file_date.format("%Y-%m-%d").to_string();

            // Group files by date and store the parent directory and oldest creation time
            let parent_dir = path.parent().unwrap().to_path_buf();
            file_groups.entry(date_str)
                .and_modify(|(_, files, oldest_time)| {
                    files.push(path.to_path_buf());
                    if created < *oldest_time {
                        *oldest_time = created_with_offset;
                    }
                })
                .or_insert((parent_dir, vec![path.to_path_buf()], created_with_offset));
        }
    }

    // Create zip files for each group
    for (date, (parent_dir, files, oldest_time)) in file_groups {

        // Deals with naming the zip file
        let zip_file_path: PathBuf = get_new_zip_path(&date, parent_dir, &search_string);
        
        // Create the zip file with the correct creation time
        let oldest_time = FileTime::from_unix_time(oldest_time.timestamp(), 0);
        
        // Create an empty file with the correct creation time
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&zip_file_path)?;
        
        // Use a closure to handle zip file creation and return a Result
        let create_zip = || -> std::io::Result<()> {
            let mut zip = ZipWriter::new(file);

            for file_path in &files {
                let file_name = file_path.file_name().unwrap().to_str().unwrap();
                zip.start_file(file_name, FileOptions::default())?;
                let mut f = File::open(file_path)?;
                std::io::copy(&mut f, &mut zip)?;
            }

            zip.finish()?;
            Ok(())
        };

        // Only remove files if zip creation is successful
        match create_zip() {
            Ok(_) => {
                info!("Created zip file: '{}'", zip_file_path.display());
                
                // Set the modification time of the zip file again (creation time should remain unchanged)
                filetime::set_file_mtime(&zip_file_path, oldest_time)?;
                
                // Remove original files
                for file_path in files {
                    if let Err(e) = fs::remove_file(&file_path) {
                        error!("Error removing file {}: {}", file_path.display(), e);
                    } else {
                        info!("Removed file: '{}'", file_path.display());
                    }
                }
            },
            Err(e) => {
                error!("Error creating zip file {}: {}", zip_file_path.display(), e);
                // Try to remove the partially created zip file
                if let Err(remove_err) = fs::remove_file(&zip_file_path) {
                    error!("Error removing partial zip file: {}", remove_err);
                }
            }
        }
    }

    Ok(())
}

fn get_new_zip_path (date: &str, basepath: PathBuf, search_string: &str) -> PathBuf{

    let mut zip_int = 1;
    let mut found: bool = false;
    let mut zip_file_path = basepath.clone(); // Initialize it with basepath

    while !found {
        let zip_file_name = format!("{}_{}-{}.zip", date, search_string, zip_int);
        zip_file_path = basepath.join(&zip_file_name);

        if Path::new(&zip_file_path).exists() {
            zip_int += 1;
        } else {
            found = true;
        }
    }

    return zip_file_path

}

fn move_files_except_today(
    source_dir: &str,
    dest_dir: &str,
    filename_contains: &str
) -> std::io::Result<()> {
    let today = Local::now().date_naive();

    create_status_file(&source_dir,&filename_contains,&dest_dir,today)?;

    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            let filename = path.file_name().unwrap().to_string_lossy();
            
            // Get the file's creation date
            let metadata = fs::metadata(&path)?;
            let created: DateTime<Utc> = metadata.created()?.into();

            // Convert to local time with offset
            let local_time = created.with_timezone(&Local);
            let offset = local_time.offset().fix();
            let created_with_offset = created.with_timezone(&offset);
            let file_date = created_with_offset.date_naive();

            if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {

                // Check if the extension is a log, txt, or zip file
                if extension == "log" || extension == "txt" || extension == "zip" {

                    // Check if the file was created today
                    if file_date == today {
                        info!("Not moving file: {:?} because it was made today", path.file_name().unwrap());
                        continue;
                    }

                    // Check if the filename contains the specified string
                    if filename.contains(filename_contains) {
                        let new_path = Path::new(dest_dir).join(path.file_name().unwrap());
                        fs::rename(&path, &new_path)?;
                        info!("Moved file: '{}' to '{}'", path.display(), new_path.display());

                    }
                }
            }
        }
    }

    Ok(())
}

fn create_status_file (source_dir: &str, filename_contains: &str, dest_dir: &str, today: NaiveDate) -> std::io::Result<()>{

    // Declare some const for naming and filling file with content
    const FILE_SUFFIX: &str = "files have been moved.status";
    const MESSAGE_TEMPLATE1: &str = "files older than";
    const MESSAGE_TEMPLATE2: &str = "were moved to";

    // Create the path and name, join them together
    let file_name = format!("{} {}", filename_contains, FILE_SUFFIX);
    let content = format!("'{}\\*{}*.[log|txt|zip]' {} {} {} '{}'", source_dir, filename_contains, MESSAGE_TEMPLATE1, today, MESSAGE_TEMPLATE2, dest_dir);
    let file_path: PathBuf = Path::new(source_dir).join(file_name);

    // Attempt to remove the file if it exists
    match remove_file(&file_path) {
        Ok(_) => (),
        Err(e) if e.kind() == ErrorKind::NotFound => (), // File doesn't exist, which is fine
        Err(e) => return Err(e), // Other errors should be propagated
    }

    // Make the status file with content
    let mut file = File::create(&file_path)?;
    file.write_all(content.as_bytes())?;
    info!("Created a status file at '{}'", file_path.display());

    Ok(())

}

fn config_application_setting_checker (dir_retentionindays: &u64) -> bool {

        // Retentionindays should not be 0
        if *dir_retentionindays < 1 {
            error!("[application]logretentionindays setting should be a number between 2-365. Application will now close.");
            return false
        }
    
        // Retentionindays should not be greater then 365
        if *dir_retentionindays > 365 {
            error!("[application]logretentionindays setting should be a number between 2-365. Application will now close.");
            return false
        }

        // No issues with the config file. No ID 10T errors here!
        true
}

fn config_directory_setting_checker (dir_path: &str, dir_filenamecontains: &str, dir_retentionindays: &u64) -> bool {

    // path should be a directory
    let path = Path::new(&dir_path);
    if !path.is_dir() {
        warn!("[directory]path setting should be an existing directory but is set to '{}'.", dir_path);
        return false
    }

    // filenamecontains should not be blank
    if dir_filenamecontains == "" {
        warn!("[directory]filenamecontains setting should not be blank for the path '{}'", dir_path);
        return false
    }

    // filenamecontains should not contain a '_'
    if dir_filenamecontains.contains('_') {
        warn!("[directory]filenamecontains setting should not contain a '_' for the path '{}'", dir_path);
        return false
    }

    // Retentionindays should not be 0
    if *dir_retentionindays == 0 {
        warn!("[directory]retentionindays setting should be a number between 1-365 for Path '{}', Name '{}'", dir_path, dir_filenamecontains);
        return false
    }

    // Retentionindays should not be greater then 365
    if *dir_retentionindays > 365 {
        warn!("[directory]retentionindays setting should be a number between 1-365 for Path '{}', Name '{}'", dir_path, dir_filenamecontains);
        return false
    }

    // No issues with the config file. No ID 10T errors here!
    true
}

fn main() {

    // Load config file
    match load_config("config.toml") {
        Ok(config_file) => {

            // Starting Tasks
            let start_time = starttask(&config_file.application.logretentionindays);

            // For Each each directory imported from config file
            for dir in &config_file.directories.directory {
                //debug!("Path: {}, FileName {}, Retention: {}, Compress {}, MoveTo {}", dir.path, dir.filenamecontains, dir.retentionindays, dir.compress, dir.movetopath);
                
                // Verify the config settings
                if config_directory_setting_checker(&dir.path, &dir.filenamecontains, &dir.retentionindays) {
                    info!("Directory Config settings are correct for Path '{}', Name '{}'", dir.path, dir.filenamecontains);
                }else{
                    continue;
                }

                // Remove old log files
                info!("Removing files with a date modified older then {} days for FilePath '{}\\*{}*.[log|txt|zip]'", dir.retentionindays, dir.path, dir.filenamecontains);
                match remove_old_files(&dir.path, &dir.filenamecontains, &dir.retentionindays) {
                    Ok(_) => info!("Completed file retention"),
                    Err(e) => error!("There was an issue removing the files: {}", e),
                
                }

                // Daily Compress log files
                if dir.compress == true {
                    info!("Compressing files older then today for FilePath '{}\\*{}*.[log|txt]'", dir.path, dir.filenamecontains);
                    match group_and_compress_files(&dir.path, &dir.filenamecontains) {
                        Ok(_) => info!("Completed file compression"),
                        Err(e) => error!("There was an issue compressing the files: {}", e),

                    }
                }else {
                    info!("Skipping File Compression for FilePath '{}\\*{}*.[log|txt]' because compress setting is false", dir.path, dir.filenamecontains)
                }

                // Move to path if it is set and exists
                let path = Path::new(&dir.movetopath);
                if path.is_dir() {
                   
                    // Remove log files to movetopath
                    info!("Moving files to '{}' older then today from FilePath '{}\\*{}*.[log|txt|zip]'", dir.movetopath, dir.path, dir.filenamecontains);
                    match move_files_except_today(&dir.path, &dir.movetopath, &dir.filenamecontains) {
                        Ok(_) => info!("Completed file move"),
                        Err(e) => error!("There was an issue moving the files: {}", e),

                    }

                    // Remove old log files in movetopath
                    info!("Removing files with a date modified older then {} days for FilePath '{}\\*{}*.[log|txt|zip]'", dir.retentionindays, dir.movetopath, dir.filenamecontains);
                    match remove_old_files(&dir.movetopath, &dir.filenamecontains, &dir.retentionindays) {
                        Ok(_) => info!("Completed file retention"),
                        Err(e) => error!("There was an issue removing the files: {}", e),
                    
                    }

                } else {
                    info!("Skipping moving logs to movetopath setting because directory does not exist or blank.");

                }

            }

                // Stopping Tasks
                endtasks(start_time);

        }
        Err(e) => println!("Failed to load config: {}", e),

    }
   

}