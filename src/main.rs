extern crate simplelog;
extern crate chrono;

use simplelog::*;
use log::{info, error};
use std::{fs::{self, File}, path::{Path, PathBuf}, time::{Duration, Instant, SystemTime}};
use chrono::*;
use time::UtcOffset;
use serde::Deserialize;
use toml;
use std::process;
use log_rc::*;

const APP_NAME: &str = "LogRC";
const LOG_NAME: &str = "LogRetentionandCompression";

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
    init_logger(LOG_NAME).expect("Failed to initialize logger");
    
    // Start up text
    info!("Starting up {}...", APP_NAME);

    // Verify Application config settings
    if config_application_setting_checker(&days) {
        
        // Remove old Application log files
        info!("Application log retention: {} days", days);
        remove_old_files("log", LOG_NAME, days).expect("Failed to remove application logs past retention");
        
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

    // Need to clone this for the whle loop
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

fn main() {

    // Load config file
    let config_file_name = format!("{}.toml", APP_NAME);
    match load_config(&config_file_name) {
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
        Err(e) => println!("Failed to load toml config: {}", e),

    }
   

}