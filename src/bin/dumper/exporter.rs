use std::fs;
use std::path::{Path, PathBuf};

use crate::Library;

use super::LibraryCache;

/// Determine the master image root directory ("Masters" or library root)
pub fn get_master_root(library_abs: &Path) -> PathBuf {
    let masters_dir = library_abs.join("Masters");
    if masters_dir.is_dir() {
        masters_dir
    } else {
        library_abs.to_path_buf()
    }
}

pub fn get_version_image_path(library_path: &str, version_uuid: &str, file_name: &str) -> PathBuf {
    let subdir = &version_uuid[0..2];
    Path::new(library_path)
        .join("Database")
        .join("Versions")
        .join(subdir)
        .join(format!("{version_uuid}.apversion"))
        .join(file_name)
}

/// The main export entry point, moved from process_export in main.rs
pub fn process_export(args: &super::ExportArgs) {
    let library_abs = fs::canonicalize(&args.path)
        .expect("Failed to resolve absolute path to library");

    let mut library = Library::new(&args.path);

    let cache_path = {
        use std::hash::{Hasher, Hash};
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        args.path.hash(&mut hasher);
        let hash = hasher.finish();
        PathBuf::from(format!("/tmp/aplib_cache_{hash:x}.bin"))
    };
    let cache = LibraryCache::new_or_load(&mut library, &cache_path);

    let out_dir = args.out_dir.as_deref().unwrap_or(".");
    if args.dryrun {
        println!("mkdir -p '{}'", out_dir);
    } else {
        fs::create_dir_all(out_dir).expect("Failed to create output directory");
    }

    // Now use cache.version_map, cache.master_map, etc. for all lookups
    if args.albums {
        // ...move the albums export logic here from main.rs...
    } else if args.folders {
        // ...move the folders export logic here from main.rs...
    } else if args.masters {
        // ...move the masters export logic here from main.rs...
    } else if args.versions {
        // ...move the versions export logic here from main.rs...
    } else {
        eprintln!("Specify --albums, --folders, --masters, or --versions for export.");
    }
}