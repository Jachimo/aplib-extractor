/*
 * Copyright (C) 2015-2025 Hubert Figuière
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use std::collections::HashMap;
use std::fs;
use std::io::stderr;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use num_traits::ToPrimitive;
use pbr::ProgressBar;
use serde_json;

use aplib::audit::{Report, Reporter};
use aplib::AplibObject;
use aplib::Keyword;
use aplib::Library;
use aplib::ModelInfo;
use aplib::StoreWrapper;
use aplib::{AlbumSubclass, PROGRESS_NONE};

mod exporter;
mod tree;

#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    Dump(CommandArgs),
    Audit(CommandArgs),
    List(CommandArgs),
    Tree(tree::TreeArgs),
    Export(ExportArgs),
}

#[derive(Clone, Debug, Parser)]
struct CommandArgs {
    #[arg(long)]
    all: bool,
    #[arg(long)]
    albums: bool,
    #[arg(long)]
    versions: bool,
    #[arg(long)]
    masters: bool,
    #[arg(long)]
    folders: bool,
    #[arg(long)]
    keywords: bool,
    #[arg(long)]
    volumes: bool,
    path: String,
}

#[derive(Clone, Debug, Parser)]
struct ExportArgs {
    #[arg(long)]
    all: bool,
    #[arg(long)]
    albums: bool,
    #[arg(long)]
    folders: bool,
    #[arg(long)]
    masters: bool,
    #[arg(long)]
    versions: bool,
    #[arg(long)]
    out_dir: Option<String>,
    #[arg(long)]
    dryrun: bool,
    #[arg(long)]
    debug: bool,
    path: String,
}

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct LibraryCache {
    version_map: HashMap<String, aplib::Version>,
    master_map: HashMap<String, aplib::Master>,
    album_map: HashMap<String, aplib::Album>,
    folder_map: HashMap<String, aplib::Folder>,
}

impl LibraryCache {
    fn new_or_load(library: &mut Library, cache_path: &Path) -> Self {
        // Try to load cache from disk
        if let Ok(file) = std::fs::File::open(cache_path) {
            println!(
                "Found existing cache file at {}. Reading / deserializing...",
                cache_path.display()
            );
            match serde_json::from_reader(file) {
                Ok(cache) => {
                    println!("Loaded cache from {}", cache_path.display());
                    return cache;
                }
                Err(e) => {
                    println!("Failed to deserialize cache, rebuilding... Error: {e:?}");
                }
            }
        } else {
            println!("No cache found at {}, building cache...", cache_path.display());
        }

        // Build cache with progress bars
        println!("Building version cache...");

        library.load_versions::<fn(u64) -> bool>(None); // dummy closure type to make rust stop complaining

        let versions = library.versions();
        let mut version_map = HashMap::new();
        for version_uuid in versions {
            if let Some(StoreWrapper::Version(version)) = library.get(version_uuid) {
                version_map.insert(version_uuid.to_owned(), (**version).clone());
            }
        }

        println!("Building master cache...");
        library.load_masters(PROGRESS_NONE);
        let masters = library.masters();
        let mut pb = ProgressBar::on(stderr(), masters.len() as u64);
        let mut master_map = HashMap::new();
        for master_uuid in masters {
            if let Some(StoreWrapper::Master(master)) = library.get(master_uuid) {
                master_map.insert(master_uuid.to_owned(), (**master).clone());
            }
            pb.inc();
        }
        pb.finish();

        println!("Building album cache...");
        library.load_albums(PROGRESS_NONE);
        let albums = library.albums();
        let mut pb = ProgressBar::on(stderr(), albums.len() as u64);
        let mut album_map = HashMap::new();
        for album_uuid in albums {
            if let Some(StoreWrapper::Album(album)) = library.get(album_uuid) {
                album_map.insert(album_uuid.to_owned(), (**album).clone());
            }
            pb.inc();
        }
        pb.finish();

        println!("Building folder cache...");
        library.load_folders(PROGRESS_NONE);
        let folders = library.folders();
        let mut pb = ProgressBar::on(stderr(), folders.len() as u64);
        let mut folder_map = HashMap::new();
        for folder_uuid in folders {
            if let Some(StoreWrapper::Folder(folder)) = library.get(folder_uuid) {
                folder_map.insert(folder_uuid.to_owned(), (**folder).clone());
            }
            pb.inc();
        }
        pb.finish();

        let cache = Self {
            version_map,
            master_map,
            album_map,
            folder_map,
        };
        // Persist cache to disk
        if let Ok(file) = std::fs::File::create(cache_path) {
            if let Err(e) = serde_json::to_writer(file, &cache) {
                eprintln!("Failed to write cache: {e}");
            } else {
                println!("Wrote cache to {}", cache_path.display());
            }
        } else {
            eprintln!("Failed to create cache file {}", cache_path.display());
        }
        cache
    }
}

fn main() {
    let args = Args::parse();

    match args.command {
        Command::Dump(_) => process_dump(&args),
        Command::Audit(_) => process_audit(&args),
        Command::List(_) => process_list(&args),
        Command::Tree(args) => tree::process_tree(&args),
        Command::Export(args) => exporter::process_export(&args),
    };
}

fn process_list(args: &Args) {
    if let Command::List(args) = &args.command {
        let mut library = Library::new(&args.path);
        {
            let version = library.library_version();
            if version.is_err() {
                println!("Invalid library");
                return;
            }
        }
        if args.volumes {
            library.load_volumes(PROGRESS_NONE);
            let volumes = library.volumes();
            for uuid in volumes {
                if uuid.is_empty() {
                    continue;
                }
                if let Some(StoreWrapper::Volume(volume)) = library.get(uuid) {
                    let name = match &volume.volume_name {
                        Some(n) => n.as_str(),
                        None => "",
                    };
                    println!("{name}\t{uuid}");
                }
            }
        } else if args.albums {
            library.load_albums(PROGRESS_NONE);
            let albums = library.albums();
            for uuid in albums {
                if uuid.is_empty() {
                    continue;
                }
                if let Some(StoreWrapper::Album(album)) = library.get(uuid) {
                    let name = match &album.name {
                        Some(n) => n.as_str(),
                        None => "",
                    };
                    println!("{name}\t{uuid}");
                }
            }
        } else if args.masters {
            library.load_masters(PROGRESS_NONE);
            let masters = library.masters();
            for master_uuid in masters {
                if master_uuid.is_empty() {
                    continue;
                }
                if let Some(master_path) = library.resolve_master_path(master_uuid) {
                    println!("{master_path}");
                } else {
                    eprintln!("Can't resolve master path for {master_uuid}");
                }
            }
        } else {
            println!("Specify --albums, --volumes, --masters, etc.");
        }
    }
}

fn print_report(report: &Report) {
    println!("+---- Ignored {}", report.ignored_count());
    let mut ignored: Vec<&String> = report.get_ignored().iter().collect();
    ignored.sort();
    for key in ignored {
        println!("    +- {key}");
    }
    println!("+---- Skipped {}", report.skipped_count());
    let mut skipped: Vec<&String> = report.get_skipped().keys().collect();
    skipped.sort();
    for key in skipped {
        let reason = &report.get_skipped()[key];
        println!("    +- {key} ({reason:?})");
    }
}

fn process_audit(args: &Args) {
    if let Command::Audit(args) = &args.command {
        let mut library = Library::new(&args.path);

        let auditor = Reporter::new();
        library.set_auditor(Some(auditor));

        {
            let version = library.library_version();
            if version.is_err() {
                println!("Invalid library");
                return;
            }
        }
        library.load_volumes(PROGRESS_NONE);
        library.load_folders(PROGRESS_NONE);
        library.load_albums(PROGRESS_NONE);
        library.load_masters(PROGRESS_NONE);
        library.load_versions(PROGRESS_NONE);

        println!("Audit:");
        let auditor = library.auditor().unwrap();
        println!("Parsed {}", auditor.parsed_count());
        println!("+-----------------------------");
        for (key, report) in auditor.get_parsed() {
            if report.skipped_count() > 0 || report.ignored_count() > 0 {
                println!("| {key} ");
                print_report(report);
            }
        }
        println!("+-----------------------------");
        println!("Skipped {}", auditor.skipped_count());
        for key in auditor.get_skipped().keys() {
            println!("| {key} ");
        }
        println!("Ignored {}", auditor.ignored_count());
        for key in auditor.get_ignored() {
            println!("| {key} ");
        }
    } else {
        unreachable!()
    }
}

/// print the keywords with indentation for the hierarchy
fn print_keywords(keywords: &[Keyword], indent: &str) {
    for keyword in keywords {
        if !keyword.is_valid() {
            continue;
        }
        // Use a &str to avoid needing type inference for the inner Option<T>
        let name = keyword.name.as_str();
        let uuid = keyword.uuid().as_ref().map(|s| s.as_str()).unwrap_or("");
        let parent = keyword.parent().clone().unwrap_or_default();
        println!("| {uuid:<26} | {parent:<26} | {indent}{name}");
        if let Some(children) = keyword.children.as_ref() {
            let new_indent = if indent.is_empty() {
                String::from("+- ") + indent
            } else {
                String::from("\t") + indent
            };
            print_keywords(children, &new_indent);
        }
    }
}

fn dump_keywords(library: &mut Library) {
    if let Some(keywords) = library.list_keywords() {
        println!("{} Keywords:", keywords.len());
        println!("| uuid                       | parent                   | name");
        println!("+----------------------------+--------------------------+---------------------");
        print_keywords(&keywords, "");
    } else {
        println!("No keywords found.");
    }
}

fn process_dump(args: &Args) {
    if let Command::Dump(args) = &args.command {
        let mut library = Library::new(&args.path);

        // Use a cache file (in /tmp or similar), based on library path hash
        // This is to make life bearable if the Aperture library is on a network share
        let cache_path = {
            use std::hash::{Hasher, Hash};
            use std::collections::hash_map::DefaultHasher;
            let mut hasher = DefaultHasher::new();
            args.path.hash(&mut hasher);
            let hash = hasher.finish();
            PathBuf::from(format!("/tmp/aplib_cache_{hash:x}.bin"))
        };
        let cache = LibraryCache::new_or_load(&mut library, &cache_path);

        {
            if let Ok(version) = library.library_version() {
                println!("Version {version}");
            } else {
                println!("Version not found.");
                return;
            }
        }

        let model_info = library.get_model_info().unwrap();
        let library_abs = fs::canonicalize(&args.path).expect("Failed to resolve absolute path to library");

        println!("model info");
        println!("\tDB version: {}", model_info.db_version.unwrap_or(0));
        println!(
            "\tDB minor version: {}",
            model_info.db_minor_version.unwrap_or(0)
        );
        println!(
            "\tDB back compat: {}",
            model_info.db_minor_back_compatible_version.unwrap_or(0)
        );
        println!(
            "\tProject version: {}",
            model_info.project_version.unwrap_or(0)
        );
        println!(
            "\tCreation date: {}",
            model_info
                .create_date
                .as_ref()
                .unwrap_or(&String::from("NONE"))
        );
        println!(
            "\tImageIO: {} Camera RAW: {}",
            model_info
                .image_io_version
                .as_ref()
                .unwrap_or(&String::from("NONE")),
            model_info
                .raw_camera_bundle_version
                .as_ref()
                .unwrap_or(&String::from("NONE"))
        );

        if args.all || args.volumes {
            dump_volumes(&mut library);
        }
        if args.all || args.folders {
            dump_folders(&mut library, &cache);
        }
        if args.all || args.albums {
            dump_albums(&mut library, &cache);
        }
        if args.all || args.keywords {
            dump_keywords(&mut library);
        }

        if args.all || args.masters {
            dump_masters(&model_info, &mut library, &library_abs, &cache);
        }
        if args.all || args.versions {
            dump_versions(&model_info, &cache);
        }
    } else {
        unreachable!()
    }
}

fn dump_volumes(library: &mut Library) {
    let mut pb = ProgressBar::on(stderr(), 1);
    pb.tick_format("|/-\\");

    library.load_volumes(Some(&mut |_: u64| {
        pb.tick();
        true
    }));
    pb.finish();

    let volumes = library.volumes();
    println!("{} Volumes:", volumes.len());

    println!("| Name                   | uuid                   | Disk UUID                            | id   |");
    println!("+------------------------+------------------------+--------------------------------------+------+");
    for uuid in volumes {
        if uuid.is_empty() {
            continue;
        }
        match library.get(uuid) {
            Some(StoreWrapper::Volume(volume)) => {
                let name = volume.volume_name.as_ref().unwrap();
                let uuid = volume.uuid().as_ref().unwrap();
                let disk_uuid = volume.disk_uuid.clone().unwrap_or_default();
                let model_id = volume.model_id();
                println!(
                    "| {name:<22} | {uuid:<22} | {disk_uuid:<36} | {model_id:>4} |",
                )
            }
            _ => {
                println!("Folder not found.");
            }
        }
    }
}

fn dump_folders(library: &mut Library, cache: &LibraryCache) {
    let mut pb = ProgressBar::on(stderr(), 1);
    pb.tick_format("|/-\\");

    library.load_folders(Some(&mut |_: u64| {
        pb.tick();
        true
    }));
    pb.finish();

    let folders = library.folders();
    println!("{} Folders:", folders.len());
    println!("| Name                   | uuid                   | parent                 | impl album                            | type      | model id | path");
    println!("+------------------------+------------------------+------------------------+---------------------------------------+-----------+----------+----------");
    for folder_uuid in folders {
        if folder_uuid.is_empty() {
            continue;
        }
        if let Some(folder) = cache.folder_map.get(folder_uuid) {
            let name = folder.name.as_ref().unwrap();
            let uuid = folder.uuid().as_ref().unwrap();
            let parent_uuid = folder.parent().clone().unwrap_or_default();
            let implicit_album_uuid = folder.implicit_album_uuid.clone().unwrap_or_default();
            let path = folder.path.as_ref().unwrap();
            let folder_type = folder.folder_type.as_ref().unwrap_or(&aplib::Type::Invalid);
            let folder_type_str = match *folder_type {
                aplib::Type::Invalid => "*",
                aplib::Type::Folder => "Folder",
                aplib::Type::Project => "Project",
            };

            let folder_type_num = folder
                .folder_type
                .as_ref()
                .and_then(num_traits::ToPrimitive::to_i32)
                .unwrap_or(0);
            println!(
                "| {:<22} | {:<22} | {:<22} | {:<37} | {:<7}{:>2} | {:>8} | {}",
                name,
                uuid,
                parent_uuid,
                implicit_album_uuid,
                folder_type_str,
                folder_type_num,
                folder.model_id(),
                path
            )
        } else {
            println!("folder {folder_uuid} not found");
        }
    }
}

fn dump_albums(library: &mut Library, cache: &LibraryCache) {
    let mut pb = ProgressBar::on(stderr(), 1);
    pb.tick_format("|/-\\");

    library.load_albums(Some(&mut |_: u64| {
        pb.tick();
        true
    }));
    pb.finish();

    let albums = library.albums();
    println!("{} Albums:", albums.len());
    println!("| uuid                                  | parent (fldr)              | query (fldr)               | type | class      | model id | name");
    println!("+---------------------------------------+----------------------------+----------------------------+------+------------+----------+-----");
    for album_uuid in albums {
        if album_uuid.is_empty() {
            continue;
        }
        if let Some(album) = cache.album_map.get(album_uuid) {
            let name = album.name.clone().unwrap_or_default();
            let uuid = album.uuid().as_ref().unwrap();
            let parent = album.parent().clone().unwrap_or_default();
            let query_folder_uuid = album.query_folder_uuid.clone().unwrap_or_default();
            let album_class = album.subclass.unwrap_or_default();
            let album_class_num = album
                .subclass
                .as_ref()
                .and_then(AlbumSubclass::to_i32)
                .unwrap_or(0);
            println!(
                "| {uuid:<37} | {parent:<26} | {query_folder_uuid:<26} | {:>4} | {album_class:<8?}{album_class_num:>2} | {:>8} | {name}",
                album.album_type.unwrap_or(0),
                album.model_id(),
            )
        } else {
            println!("album {album_uuid} not found");
        }
    }
}

fn dump_masters(model_info: &ModelInfo, library: &mut Library, library_abs: &Path, cache: &LibraryCache) {
    let count = model_info.master_count.unwrap_or(0) as u64;
    let mut pb = ProgressBar::on(stderr(), count);

    library.load_masters(Some(&mut |inc: u64| {
        pb.add(inc);
        true
    }));
    pb.finish();

    let masters = library.masters();
    let master_root = exporter::get_master_root(library_abs); // Consistent w/ export
    println!("{} Masters:", masters.len());
    println!("| uuid                   | project                | alternate              | mtyp | subt  | orig | path | exists");
    println!("+------------------------+------------------------+------------------------+------+-------+-----------------------+--------");
    for master_uuid in masters {
        if master_uuid.is_empty() {
            continue;
        }
        if let Some(master) = cache.master_map.get(master_uuid) {
            let uuid = master.uuid().as_ref().unwrap();
            let parent = master.parent().as_ref().unwrap();
            let image_path = master.image_path.as_ref().unwrap();
            let alternate = master.alternate_master.clone().unwrap_or_default();
            let mtype = master.master_type.clone().unwrap_or_default();
            let subtype = master.subtype.clone().unwrap_or_default();
            let orig_uuid = master.original_version_uuid.clone().unwrap_or_default();
            let abs_path = master_root.join(image_path);
            let exists = abs_path.exists();
            println!(
                "| {uuid:<22} | {parent:<22} | {alternate:<22} | {mtype:<4} | {subtype:<5} | {orig_uuid} | {image_path} | {exists}",
            )
        } else {
            println!("master {master_uuid} not found");
        }
    }
}

fn dump_versions(model_info: &ModelInfo, cache: &LibraryCache) {
    let count = model_info.version_count.unwrap_or(0) as u64;
    let mut pb = ProgressBar::on(stderr(), count);

    pb.finish();

    println!("{} Versions:", cache.version_map.len());
    println!("| uuid                   | master                 | project                | orig  | raw   | num | name");
    println!("+------------------------+------------------------+------------------------+-------+-------+-----+------------");
    for version_uuid in cache.version_map.keys() {
        if version_uuid.is_empty() {
            continue;
        }
        if let Some(version) = cache.version_map.get(version_uuid) {
            let uuid = version.uuid().as_ref().unwrap();
            let parent = version.parent().as_ref().unwrap();
            let project_uuid = version.project_uuid.as_ref().unwrap();
            let name = version.name.as_ref().unwrap();
            let rawmaster = version.raw_master_uuid == version.master_uuid;
            let num = version
                .version_number
                .map(|v| v.to_string())
                .unwrap_or_default();

            println!(
                "| {uuid:<22} | {parent:<22} | {project_uuid:<22} | {:>5} | {rawmaster:>5} | {num:>3} | {name}",
                version.is_original.unwrap_or(false),
            )
        } else {
            println!("version {version_uuid} not found");
        }
    }
}

