/*
 * Copyright (C) 2016-2023 Hubert Figuière
 * Copyright (C) 2025-2026 the "aplib-extractor" Contributors
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use std::path::PathBuf;

/// Return the testfile path for filename
/// Test files are in the testdata/TestLibrary.aplibrary directory in the crate top level.
pub fn get_test_file_path(filename: &str) -> PathBuf {
    let mut path = PathBuf::from(file!());
    // go up two directories
    path.pop();
    path.pop();
    path.push("testdata");
    path.push("TestLibrary.aplibrary");
    path.push(filename);
    path
}

