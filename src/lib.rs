use simplelog::*;
use log::{info, error};
use std::{fs::{self, File, OpenOptions, remove_file}, path::{Path, PathBuf}, io::{Write, ErrorKind}};
use chrono::*;
use walkdir::WalkDir;
use zip::write::{FileOptions, ZipWriter};
use std::collections::HashMap;
use filetime::FileTime;

pub fn group_and_compress_files(dir_path: &str, search_string: &str) -> std::io::Result<()> {
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

pub fn get_new_zip_path (date: &str, basepath: PathBuf, search_string: &str) -> PathBuf{

    let mut zip_int = 1;
    let mut found: bool = false;

    // Need to clone this for the while loop
    let mut zip_file_path = basepath.clone();

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

pub fn move_files_except_today(
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

pub fn create_status_file (source_dir: &str, filename_contains: &str, dest_dir: &str, today: NaiveDate) -> std::io::Result<()>{

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

pub fn config_application_setting_checker (dir_retentionindays: &u64) -> bool {

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

pub fn config_directory_setting_checker (dir_path: &str, dir_filenamecontains: &str, dir_retentionindays: &u64) -> bool {

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