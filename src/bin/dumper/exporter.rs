use std::fs;
use std::path::{Path, PathBuf};

use crate::Library;
use super::LibraryCache;

/// Represents a single exportable image (master, versions, and metadata)
#[derive(Debug)]
pub struct ExportJob {
    pub master_uuid: String,
    pub master_path: PathBuf,
    pub master_filename: String,
    pub version_uuids: Vec<String>,
    pub version_paths: Vec<PathBuf>,
    pub version_filenames: Vec<String>,
    pub sidecar_filename: String,
}

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

/// Build a list of export jobs for all masters and their versions
pub fn build_export_jobs(
    library: &Library,
    cache: &LibraryCache,
    library_abs: &Path,
    out_dir: &Path,
) -> Vec<ExportJob> {
    let mut jobs = Vec::new();
    let master_root = get_master_root(library_abs);

    for (master_uuid, master) in &cache.master_map {
        // Master file info
        let master_path = master_root.join(master.image_path.as_ref().unwrap());
        let master_filename = format!("{}_master{}", master_uuid, master_path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default());

        // Find all versions for this master
        let mut version_uuids = Vec::new();
        let mut version_paths = Vec::new();
        let mut version_filenames = Vec::new();

        for (version_uuid, version) in &cache.version_map {
            if version.master_uuid.as_ref() == Some(master_uuid) {
                version_uuids.push(version_uuid.clone());
                let version_file = version.file_name.clone().unwrap_or_default();
                let version_path = get_version_image_path(
                    library_abs.to_str().unwrap(),
                    version_uuid,
                    &version_file,
                );
                let version_filename = format!(
                    "{}_version_{}{}",
                    master_uuid,
                    version_uuid,
                    Path::new(&version_file)
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default()
                );
                version_paths.push(version_path);
                version_filenames.push(version_filename);
            }
        }

        // Sidecar filename (JSON for now)
        let sidecar_filename = format!("{}_meta.json", master_uuid);

        jobs.push(ExportJob {
            master_uuid: master_uuid.clone(),
            master_path,
            master_filename,
            version_uuids,
            version_paths,
            version_filenames,
            sidecar_filename,
        });
    }
    jobs
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
    let out_dir = Path::new(out_dir);
    if args.dryrun {
        println!("mkdir -p '{}'", out_dir.display());
    } else {
        fs::create_dir_all(out_dir).expect("Failed to create output directory");
    }

    if args.albums {
        println!("Exporting albums is not yet implemented.");
    } else if args.folders {
        println!("Exporting folders is not yet implemented.");
    } else if args.masters {
        println!("Exporting masters is not yet implemented.");
    } else if args.versions {
        println!("Exporting versions is not yet implemented.");
    } else if args.all {
        // Build export jobs
        let jobs = build_export_jobs(&library, &cache, &library_abs, out_dir);
        println!("Prepared {} export jobs (one per master).", jobs.len());
        // TODO: For now, just print a summary of each job
        for job in &jobs {
            println!(
                "Master: {} -> {}",
                job.master_path.display(),
                out_dir.join(&job.master_filename).display()
            );
            for (v_uuid, v_path, v_file) in itertools::izip!(
                &job.version_uuids,
                &job.version_paths,
                &job.version_filenames
            ) {
                println!(
                    "  Version: {} -> {}",
                    v_path.display(),
                    out_dir.join(v_file).display()
                );
            }
            println!("  Sidecar: {}", out_dir.join(&job.sidecar_filename).display());
        }
        // TODO: actual file copying and sidecar writing
    } else {
        eprintln!("Specify --albums, --folders, --masters, --versions, or --all for export.");
    }
}