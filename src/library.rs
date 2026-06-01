/*
 * Copyright (C) 2016-2025 Hubert Figuière
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::stderr;
use std::path::{Path, PathBuf};
use std::time::Instant;

use once_cell::unsync::OnceCell;
use pbr::ProgressBar;
use plist::Value;

use crate::album::Album;
use crate::audit::{audit_get_str_value, Report, Reporter, SkipReason};
use crate::folder::Folder;
use crate::keyword::{parse_keywords, Keyword};
use crate::master::Master;
use crate::plutils;
use crate::store;
use crate::version::Version;
use crate::volume::Volume;
use crate::{AplibObject, PlistLoadable, SqliteLoadable};

// This is based on Aperture db_version = 110
const INFO_PLIST: &str = "Info.plist";
const BUNDLE_IDENTIFIER: &str = "com.apple.Aperture.library";

// in Database
//pub const DATAMODEL_VERSION_PLIST: &str = "DataModelVersion.plist";
pub const KEYWORDS_PLIST: &str = "Keywords.plist";
pub const ALBUMS_DIR: &str = "Albums";
pub const FOLDERS_DIR: &str = "Folders";
pub const VOLUMES_DIR: &str = "Volumes";
pub const VERSIONS_BASE_DIR: &str = "Versions";

// for progress bar
pub const PROGRESS_NONE: Option<fn(u64) -> bool> = None;

/// Info of the library data model
pub struct ModelInfo {
    pub is_iphoto_library: Option<bool>,
    pub db_version: Option<i64>,
    pub db_minor_back_compatible_version: Option<i64>,
    pub db_minor_version: Option<i64>,
    pub db_uuid: Option<String>,
    pub create_date: Option<String>,
    pub image_io_version: Option<String>,
    pub raw_camera_bundle_version: Option<String>,
    pub touched_by_aperture: Option<bool>,
    pub master_count: Option<i64>,
    pub version_count: Option<i64>,
    pub project_compat_back_to_version: Option<i64>,
    pub project_version: Option<i64>,
}

impl ModelInfo {
    fn parse(plist: &Value) -> Option<ModelInfo> {
        use crate::plutils::{get_bool_value, get_int_value, get_str_value};

        match *plist {
            Value::Dictionary(ref dict) => Some(ModelInfo {
                db_uuid: get_str_value(dict, "databaseUuid"),
                db_minor_back_compatible_version: get_int_value(
                    dict,
                    "DatabaseCompatibleBackToMinorVersion",
                ),
                db_minor_version: get_int_value(dict, "DatabaseMinorVersion"),
                db_version: get_int_value(dict, "DatabaseVersion"),
                is_iphoto_library: get_bool_value(dict, "isIPhotoLibrary"),
                create_date: get_str_value(dict, "createDate"),
                image_io_version: get_str_value(dict, "imageIOVersion"),
                raw_camera_bundle_version: get_str_value(dict, "rawCameraBundleVersion"),
                touched_by_aperture: get_bool_value(dict, "touchedByAperture"),
                master_count: get_int_value(dict, "masterCount"),
                version_count: get_int_value(dict, "versionCount"),
                project_version: get_int_value(dict, "projectVersion"),
                project_compat_back_to_version: get_int_value(
                    dict,
                    "projectCompatibleBackToVersion",
                ),
            }),
            _ => None,
        }
    }
}

/// Library is the Aperture library.
pub struct Library {
    /// The path to the .aplib bundle (the directory)
    path: PathBuf,

    /// Its version string (displayed by get info in the Finder)
    version: String,

    /// All the folders UUID
    folders: HashSet<String>,
    /// All the albums UUID
    albums: HashSet<String>,
    //    keywords: HashSet<String>,
    /// All the masters UUID
    masters: HashSet<String>,
    /// All the version UUID
    versions: HashSet<String>,
    /// All the volumes UUID
    volumes: HashSet<String>,

    /// The object store. The key is the UUID
    objects: HashMap<String, store::Wrapper>,
    /// Auditor for the audit mode.
    auditor: Option<Reporter>,
    /// Database connection
    database_conn: OnceCell<Option<rusqlite::Connection>>,
}

impl Library {
    /// Create a new library object from the exist path to
    /// the bundle directory.
    pub fn new<P>(p: P) -> Library
    where
        P: AsRef<std::path::Path>,
    {
        Library {
            path: p.as_ref().to_path_buf(),
            version: String::new(),

            folders: HashSet::new(),
            albums: HashSet::new(),
            //            keywords: HashSet::new(),
            masters: HashSet::new(),
            versions: HashSet::new(),
            volumes: HashSet::new(),

            objects: HashMap::new(),
            auditor: None,

            database_conn: OnceCell::new(),
        }
    }

    /// Set an auditor.
    pub fn set_auditor(&mut self, auditor: Option<Reporter>) {
        self.auditor = auditor;
    }
    /// Get the auditor
    pub fn auditor(&self) -> Option<&Reporter> {
        self.auditor.as_ref()
    }

    /// Get the main database from the library.
    pub fn database(&self) -> &Option<rusqlite::Connection> {
        self.database_conn.get_or_init(|| {
            let dbpath = self.path.join("Database/apdb/Library.apdb");
            let connection = rusqlite::Connection::open(dbpath);
            connection.ok()
        })
    }

    /// Store the wrapped object.
    /// Return true if the object was stored
    /// Return false if there already was an object with the same uuid
    /// or if the uuid in invalid.
    pub fn store(&mut self, obj: store::Wrapper) -> bool {
        if let Some(uuid_str) = obj.uuid() {
            self.objects.insert(uuid_str, obj).is_none()
        } else {
            false
        }
    }

    /// Get an object out of the store by UUID
    pub fn get(&self, uuid: &str) -> Option<&store::Wrapper> {
        self.objects.get(uuid)
    }

    /// Get the library version. Will parse the plist for that
    /// if needed.
    pub fn library_version(&mut self) -> Result<&String, SkipReason> {
        if self.version.is_empty() {
            let plist_path = self.build_path(INFO_PLIST, false);
            let plist = plutils::parse_plist(&plist_path);
            let audit = self.auditor.is_some();
            let mut report = if audit { Some(Report::new()) } else { None };

            match plist {
                Value::Dictionary(ref dict) => {
                    let version = audit_get_str_value(
                        dict,
                        "CFBundleShortVersionString",
                        &mut report.as_mut(),
                    );
                    if version.is_none() {
                        println!("FATAL no library version found");
                        return Err(SkipReason::NotFound);
                    }
                    self.version = version.unwrap();

                    let bundle_id =
                        audit_get_str_value(dict, "CFBundleIdentifier", &mut report.as_mut());
                    if let Some(id) = bundle_id {
                        if id != BUNDLE_IDENTIFIER {
                            if audit {
                                if let Some(ref mut r) = report {
                                    r.skip("CFBundleIdentifier", SkipReason::InvalidData);
                                }
                            }
                            println!("FATAL not a library");
                            return Err(SkipReason::InvalidData);
                        }
                    } else if audit {
                        if let Some(ref mut r) = report {
                            r.skip("CFBundleIdentifier", SkipReason::NotFound);
                        }
                        println!("FATAL no bundle identifier");
                        return Err(SkipReason::NotFound);
                    }

                    if audit {
                        if let Some(ref mut r) = report {
                            r.audit_ignored(dict, None);
                        }
                        self.auditor
                            .as_mut()
                            .unwrap()
                            .parsed(&plist_path.to_string_lossy(), report.unwrap());
                    }
                }
                _ => {
                    if audit {
                        self.auditor
                            .as_mut()
                            .unwrap()
                            .skip(&plist_path.to_string_lossy(), SkipReason::InvalidType);
                    }
                }
            }
        }
        Ok(&self.version)
    }

    /// Helper to build path relative to the library root
    fn build_path(&self, subpath: &str, _is_dir: bool) -> PathBuf {
        self.path.join(subpath)
    }

    /// Helper to find correct path for a given subdirectory (e.g., Albums or Folders)
    fn resolve_subdir(&self, subdir: &str) -> Option<PathBuf> {
        // First, try Database/<subdir>
        let db_path = self.build_path(&format!("Database/{}", subdir), true);
        if fs::read_dir(&db_path).is_ok() {
            return Some(db_path);
        } else {
            eprintln!(
                "Debug: Database/{} not found in library, trying top-level {} directory.",
                subdir, subdir
            );
        }
        // Fallback: try top-level <subdir>
        let top_path = self.build_path(subdir, true);
        if fs::read_dir(&top_path).is_ok() {
            eprintln!(
                "Debug: Using top-level {} directory at {:?}.",
                subdir, top_path
            );
            return Some(top_path);
        } else {
            eprintln!(
                "Debug: No {} directory found at Database/{} or top-level.",
                subdir, subdir
            );
        }
        None
    }

    /// Recursively list directories up to a certain depth
    fn recurse_list_directory(path: &Path, level: i32) -> Vec<PathBuf> {
        let mut list: Vec<PathBuf> = Vec::new();
        let entries = fs::read_dir(path);
        if entries.is_err() {
            eprintln!("Warning: failed to read directory {:?}", path);
            return list;
        }
        for entry in entries.unwrap() {
            if let Ok(entry) = entry {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        if level == 0 {
                            list.push(entry.path());
                        } else {
                            let mut sublist = Library::recurse_list_directory(&entry.path(), level - 1);
                            list.append(&mut sublist)
                        }
                    }
                } else {
                    eprintln!("Warning: failed to get metadata for {:?}", entry.path());
                }
            } else {
                eprintln!("Warning: failed to read entry in {:?}", path);
            }
        }
        list
    }

    /// Recursively list directories up to a certain depth and emit periodic status.
    fn recurse_list_directory_with_status(path: &Path, level: i32, label: &str) -> Vec<PathBuf> {
        let mut list: Vec<PathBuf> = Vec::new();
        let mut stack: Vec<(PathBuf, i32)> = vec![(path.to_path_buf(), level)];
        let mut dirs_visited: u64 = 0;
        let mut last_report = Instant::now();

        while let Some((dir_path, depth_left)) = stack.pop() {
            dirs_visited += 1;
            if last_report.elapsed() >= std::time::Duration::from_secs(2) {
                eprintln!("{}: visited {} directories...", label, dirs_visited);
                last_report = Instant::now();
            }

            let entries = fs::read_dir(&dir_path);
            if entries.is_err() {
                eprintln!("Warning: failed to read directory {:?}", dir_path);
                continue;
            }

            for entry in entries.unwrap() {
                if let Ok(entry) = entry {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_dir() {
                            if depth_left == 0 {
                                list.push(entry.path());
                            } else {
                                stack.push((entry.path(), depth_left - 1));
                            }
                        }
                    } else {
                        eprintln!("Warning: failed to get metadata for {:?}", entry.path());
                    }
                } else {
                    eprintln!("Warning: failed to read entry in {:?}", dir_path);
                }
            }
        }

        eprintln!(
            "{}: completed; visited {} directories, found {} leaf directories.",
            label,
            dirs_visited,
            list.len()
        );
        list
    }

    /// List items in Albums or Folders, using robust path resolution
    fn list_items_dirs(&self, subdir: &str) -> Vec<PathBuf> {
        let mut result = Vec::new();
        if let Some(ppath) = self.resolve_subdir(subdir) {
            if subdir == VERSIONS_BASE_DIR {
                result = Library::recurse_list_directory_with_status(
                    &ppath,
                    4,
                    "Scanning version directories",
                );
            } else {
                result = Library::recurse_list_directory(&ppath, 4);
            }
        }
        result
    }

    /// Load items from directory `dir` with extension `ext`
    /// and store the uuids into `set`
    fn load_items<T, F>(
        &mut self,
        dir: &str,
        ext: &str,
        set: &mut HashSet<String>,
        mut pg: Option<F>,
    ) where
        T: PlistLoadable + AplibObject,
        F: FnMut(u64) -> bool,
    {
        let file_list = self.list_recursive_items(dir, ext);
        let audit = self.auditor.is_some();
        for file in file_list {
            let mut report = if audit { Some(Report::new()) } else { None };
            if let Some(obj) = T::from_path(&file, report.as_mut()) {
                let mut store = false;
                if let Some(ref uuid) = *obj.uuid() {
                    set.insert(uuid.to_owned());
                    if audit {
                        self.auditor
                            .as_mut()
                            .unwrap()
                            .parsed(&file.to_string_lossy(), report.unwrap());
                    }
                    store = true;
                }
                if store {
                    self.store(T::wrap(obj));
                }
            } else {
                if audit {
                    self.auditor
                        .as_mut()
                        .unwrap()
                        .skip(&file.to_string_lossy(), SkipReason::ParseFailed);
                }
                eprintln!("Failed to decode object from {file:?}");
            }
            if let Some(pg) = pg.as_mut() {
                if !pg(1) {
                    println!("Cancelled");
                    return;
                }
            }
        }
    }

    /// Load albums. Once done the result it cached.
    pub fn load_albums<F: FnMut(u64) -> bool>(&mut self, pg: Option<F>) {
        if self.albums.is_empty() {
            let mut albums: HashSet<String> = HashSet::new();
            self.load_items::<Album, F>(ALBUMS_DIR, "apalbum", &mut albums, pg);
            self.albums = albums;
        }
    }

    /// Get albums uuids.
    pub fn albums(&self) -> &HashSet<String> {
        &self.albums
    }

    /// Load folders. Once done the result is cached.
    pub fn load_folders<F: FnMut(u64) -> bool>(&mut self, pg: Option<F>) {
        if self.folders.is_empty() {
            let mut folders: HashSet<String> = HashSet::new();
            self.load_items::<Folder, F>(FOLDERS_DIR, "apfolder", &mut folders, pg);
            self.folders = folders;
        }
    }

    /// Get folders uuids.
    pub fn folders(&self) -> &HashSet<String> {
        &self.folders
    }

    pub fn list_recursive_items(&self, dir: &str, ext: &str) -> Vec<PathBuf> {
        let is_versions_scan = dir == VERSIONS_BASE_DIR;
        let list = self.list_items_dirs(dir);
        let mut items = Vec::new();
        let mut processed_dirs: u64 = 0;
        let mut last_report = Instant::now();

        for dir in list {
            processed_dirs += 1;
            if is_versions_scan && last_report.elapsed() >= std::time::Duration::from_secs(2) {
                eprintln!(
                    "Indexing version files: scanned {} directories, found {} .{} files so far...",
                    processed_dirs,
                    items.len(),
                    ext
                );
                last_report = Instant::now();
            }
            let entries = fs::read_dir(&dir);
            if entries.is_err() {
                eprintln!("Warning: failed to read directory {:?}", dir);
                continue;
            }
            for entry in entries.unwrap() {
                if let Ok(entry) = entry {
                    let p = entry.path();
                    if let Some(extn) = p.extension() {
                        if extn == ext {
                            items.push(p.to_owned());
                        }
                    }
                } else {
                    eprintln!("Warning: failed to read entry in {:?}", dir);
                }
            }
        }

        if is_versions_scan {
            eprintln!(
                "Indexing version files: completed; scanned {} directories, found {} .{} files.",
                processed_dirs,
                items.len(),
                ext
            );
        }

        items
    }

    fn load_volumes_items<T, F>(&mut self, ext: &str, set: &mut HashSet<String>, mut pg: Option<F>)
    where
        T: PlistLoadable + SqliteLoadable + AplibObject,
        F: FnMut(u64) -> bool,
    {
        use rusqlite::params;

        let file_list = self.list_recursive_items(VOLUMES_DIR, ext);
        if file_list.is_empty() {
            // open the database and load from there.
            let mut objects = Vec::new();
            if let Some(conn) = self.database() {
                let query = format!("SELECT {} FROM {}", T::columns(), T::tables());
                if let Ok(mut stmt) = conn.prepare(&query) {
                    if let Ok(volumes) = stmt.query_and_then(params![], |row| T::from_row(row)) {
                        volumes
                            .into_iter()
                            .filter(|vol| vol.is_ok())
                            .for_each(|vol| {
                                let vol = vol.unwrap();
                                if let Some(uuid) = vol.uuid() {
                                    set.insert(uuid.clone());
                                    objects.push(vol);
                                }
                            });
                    }
                }
            }

            objects.into_iter().for_each(|vol| {
                self.store(T::wrap(vol));
            });

            return;
        }
        let audit = self.auditor.is_some();
        for file in file_list {
            let mut report = if audit { Some(Report::new()) } else { None };
            if let Some(obj) = T::from_path(&file, report.as_mut()) {
                let mut store = false;
                if let Some(ref uuid) = *obj.uuid() {
                    set.insert(uuid.to_owned());
                    store = true;
                    if audit {
                        self.auditor
                            .as_mut()
                            .unwrap()
                            .parsed(&file.to_string_lossy(), report.unwrap());
                    }
                }
                if store {
                    self.store(T::wrap(obj));
                }
            } else {
                if audit {
                    self.auditor
                        .as_mut()
                        .unwrap()
                        .skip(&file.to_string_lossy(), SkipReason::ParseFailed);
                }
                println!("Error decoding object from {file:?}");
            }
            if let Some(pg) = pg.as_mut() {
                if !pg(1) {
                    println!("Cancelled!");
                    break;
                }
            }
        }
    }

    fn load_versions_items<T, P>(
        &mut self,
        ext: &str,
        set: &mut HashSet<String>,
        mut pg: Option<P>,
    )
    where
        T: PlistLoadable + AplibObject,
        P: FnMut(u64) -> bool,
    {
        println!("Scanning version directories (this may take a while on large libraries)...");
        let file_list = self.list_recursive_items(VERSIONS_BASE_DIR, ext);
        let mut pb = ProgressBar::on(stderr(), file_list.len() as u64);
        pb.message("Parsing versions: ");
        pb.set_max_refresh_rate(Some(std::time::Duration::from_millis(100)));

        let audit = self.auditor.is_some();
        for file in file_list {
            let mut report = if audit { Some(Report::new()) } else { None };
            if let Some(obj) = T::from_path(&file, report.as_mut()) {
                let mut store = false;
                if let Some(ref uuid) = *obj.uuid() {
                    set.insert(uuid.to_owned());
                    store = true;
                    if audit {
                        self.auditor
                            .as_mut()
                            .unwrap()
                            .parsed(&file.to_string_lossy(), report.unwrap());
                    }
                }
                if store {
                    self.store(T::wrap(obj));
                }
            } else {
                if audit {
                    self.auditor
                        .as_mut()
                        .unwrap()
                        .skip(&file.to_string_lossy(), SkipReason::ParseFailed);
                }
                println!("Error decoding object from {file:?}");
            }
            pb.inc();
            if let Some(pg) = pg.as_mut() {
                if !pg(1) {
                    println!("Cancelled!");
                    break;
                }
            }
        }
        pb.finish();
    }

    /// Load volumess.
    pub fn load_volumes<F: FnMut(u64) -> bool>(&mut self, pg: Option<F>) {
        if self.volumes.is_empty() {
            let mut volumes: HashSet<String> = HashSet::new();
            self.load_volumes_items::<Volume, F>("apvolume", &mut volumes, pg);
            self.volumes = volumes;
        }
    }

    /// Load versions.
    pub fn load_versions<P: FnMut(u64) -> bool>(&mut self, pg: Option<P>) {
        if self.versions.is_empty() {
            let mut versions: HashSet<String> = HashSet::new();
            self.load_versions_items::<Version, P>("apversion", &mut versions, pg);
            self.versions = versions;
        }
    }

    /// Load masters.
    pub fn load_masters<F: FnMut(u64) -> bool>(&mut self, pg: Option<F>) {
        if self.masters.is_empty() {
            let mut masters: HashSet<String> = HashSet::new();
            self.load_versions_items::<Master, F>("apmaster", &mut masters, pg);
            self.masters = masters;
        }
    }

    /// Return masters uuids.
    pub fn masters(&self) -> &HashSet<String> {
        &self.masters
    }

    /// Return versions uuids.
    pub fn versions(&self) -> &HashSet<String> {
        &self.versions
    }

    /// Return volumes uuids.
    pub fn volumes(&self) -> &HashSet<String> {
        &self.volumes
    }

    /// Resolve the path of a master to it's macOS on disk location
    /// either to an existing volume or relative to the library.
    pub fn resolve_master_path(&self, uuid: &str) -> Option<String> {
        match self.get(uuid) {
            Some(crate::StoreWrapper::Master(master)) => {
                let image_path = master.image_path.as_ref()?;
                if let Some(volume_uuid) = master.file_volume_uuid.as_ref() {
                    self.get(volume_uuid).and_then(|object| {
                        if let crate::StoreWrapper::Volume(volume) = object {
                            Some(format!(
                                "/Volumes/{}/{image_path}",
                                volume.volume_name.clone().unwrap_or_else(String::default)
                            ))
                        } else {
                            None
                        }
                    })
                } else {
                    Some(format!("Masters/{image_path}"))
                }
            }
            _ => None,
        }
    }

    /// List keywords.
    pub fn list_keywords(&mut self) -> Option<Vec<Keyword>> {
        let audit = self.auditor.is_some();
        let mut report = if audit { Some(Report::new()) } else { None };

        // Build paths
        let root_path = self.build_path(KEYWORDS_PLIST, true);
        let db_label = format!("Database/{}", KEYWORDS_PLIST);
        let db_path = self.build_path(&db_label, true);

        // Helper closure to handle auditing and parsing
        let mut try_parse = |path: &Path, label: &str| -> Option<Vec<Keyword>> {
            let result = parse_keywords(path, &mut report.as_mut());
            if audit {
                let auditor = self.auditor.as_mut().unwrap();
                if result.is_some() {
                    auditor.parsed(label, report.take().unwrap());
                } else {
                    auditor.skip(label, SkipReason::ParseFailed);
                }
            }
            result
        };

        // Look for Keywords.plist in root first
        if root_path.exists() {
            return try_parse(&root_path, KEYWORDS_PLIST);
        }

        // Try the Database/Keywords.plist location
        if db_path.exists() {
            return try_parse(&db_path, &db_label);
        }
        // TODO: Look elsewhere in Library for Keywords.plist? 
        None
    }

    /// Load and return the ModelInfo for this library.
    pub fn get_model_info(&self) -> Option<ModelInfo> {
        let plist_path = self.build_path("Info.plist", false);
        let plist = plutils::parse_plist(&plist_path);
        ModelInfo::parse(&plist)
    }

    /// Identify the Masters directory, print status, and return its PathBuf.
    pub fn find_masters_dir(&self) -> Option<PathBuf> {
        let root_masters = self.build_path("Masters", true);
        if root_masters.exists() && root_masters.is_dir() {
            println!("Using Masters directory at {}", root_masters.display());
            Some(root_masters)
        } else {
            let db_masters = self.build_path("Database/Masters", true);
            if db_masters.exists() && db_masters.is_dir() {
                println!("Using Masters directory at {}", db_masters.display());
                Some(db_masters)
            } else {
                println!("No Masters directory found in library.");
                None
            }
        }
    }

    /// Identify the Versions directory, print status, and return its PathBuf.
    pub fn find_versions_dir(&self) -> Option<PathBuf> {
        let versions_dir = self.build_path("Database/Versions", true);
        if versions_dir.exists() && versions_dir.is_dir() {
            println!("Using Versions directory at {}", versions_dir.display());
            Some(versions_dir)
        } else {
            println!("No Versions directory found in library.");
            None
        }
    }

    /// Recursively find all .apmaster files in the versions directory.
    pub fn find_apmaster_files(versions_dir: &Path) -> Vec<PathBuf> {
        let mut result = Vec::new();
        if let Ok(entries) = fs::read_dir(versions_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    result.extend(Self::find_apmaster_files(&path));
                } else if let Some(ext) = path.extension() {
                    if ext == "apmaster" {
                        result.push(path);
                    }
                }
            }
        }
        result
    }

    /// Load masters by searching for .apmaster files
    pub fn load_masters_from_versions(&mut self) {

        // Identify Masters directory
        let masters_dir = match self.find_masters_dir() {
            Some(dir) => dir,
            None => {
                println!("No Masters directory found; cannot load masters.");
                return;
            }
        };
        // Identify Versions directory 
        let versions_dir = match self.find_versions_dir() {
            Some(dir) => dir,
            None => {
                println!("No Versions directory found; cannot load versions.");
                return;
            }
        };
        // Recursively find all .apmaster files in Versions directory
        let apmaster_files = Self::find_apmaster_files(&versions_dir);
        if apmaster_files.is_empty() {
            println!("No .apmaster files found in Versions directory.");
            return;
        }

        let mut masters: HashSet<String> = HashSet::new();
        let mut pb = ProgressBar::on(stderr(), apmaster_files.len() as u64);
        pb.message("Loading masters: ");
        pb.set_max_refresh_rate(Some(std::time::Duration::from_millis(100)));

        for apmaster_path in apmaster_files {
            if let Some(master) = Master::from_path(&apmaster_path, None) {
                if let Some(image_path) = master.image_path.as_ref() {
                    let master_image = masters_dir.join(image_path);
                    if master_image.exists() {
                        // Use the master UUID if present, else fallback to apmaster path as unique key
                        let uuid = if let Some(ref u) = master.uuid() {
                            u.clone()
                        } else {
                            apmaster_path.to_string_lossy().to_string()
                        };
                        // Store in object store and track UUID
                        if self.store(store::Wrapper::Master(Box::new(master))) {
                            masters.insert(uuid);
                        }
                    } else {
                        println!(
                            "Warning: Master image {} referenced by {} does not exist.",
                            master_image.display(),
                            apmaster_path.display()
                        );
                    }
                } else {
                    println!(
                        "Warning: .apmaster file {} does not contain an imagePath.",
                        apmaster_path.display()
                    );
                }
            } else {
                println!(
                    "Warning: Failed to parse .apmaster file {}.",
                    apmaster_path.display()
                );
            }
            pb.inc();
        }
        pb.finish();

        // Update the masters set for downstream code
        self.masters = masters;
    }
}
