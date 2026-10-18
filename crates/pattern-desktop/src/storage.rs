//! Filesystem operations belong to the desktop, not to pattern-core.
use pattern_core::Document;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

pub fn load(path: &Path) -> Result<Document, String> {
    let json = fs::read_to_string(path).map_err(|e| format!("Read {}: {e}", path.display()))?;
    Document::from_json(&json).map_err(|e| e.to_string())
}

/// Write a sibling temporary file, then replace the destination only after a complete write.
pub fn save(document: &Document, path: &Path) -> Result<(), String> {
    let json = document.to_json().map_err(|e| e.to_string())?;
    let name = path.file_name().ok_or("Choose a JSON file path")?;
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(".{}.tmp", std::process::id()));
    let temp = path.with_file_name(temp_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("Create {}: {e}", temp.display()))?;
    let result = (|| {
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(format!("Save {}: {error}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_round_trip_replace_and_invalid_save_preserve_existing_file() {
        let directory =
            std::env::temp_dir().join(format!("seametric-storage-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("panel.json");
        let mut doc = Document::sample_skirt();
        save(&doc, &path).unwrap();
        assert_eq!(load(&path).unwrap(), doc);
        doc.apply(pattern_core::PatternCommand::MovePoint {
            piece_id: pattern_core::PieceId(1),
            point_id: pattern_core::PointId(1),
            to: pattern_core::MmPoint::new(-7.5, 2.25),
        })
        .unwrap();
        save(&doc, &path).unwrap();
        assert_eq!(load(&path).unwrap(), doc);
        let bytes = fs::read(&path).unwrap();
        doc.schema_version = 99;
        assert!(save(&doc, &path).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::write(&path, "invalid JSON").unwrap();
        assert!(load(&path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
