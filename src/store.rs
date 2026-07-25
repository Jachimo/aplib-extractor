/*
 * Copyright (C) 2016-2023 Hubert Figuière
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use crate::master::Master;
use crate::version::Version;
use crate::volume::Volume;
use crate::AplibObject;

/// Wrap an AplibObject to key it into the store.
pub enum Wrapper {
    Master(Box<Master>),
    Version(Box<Version>),
    Volume(Box<Volume>),
    None,
}

impl Wrapper {
    /// Extract the uuid from the wrapper
    pub fn uuid(&self) -> Option<String> {
        match *self {
            Wrapper::Version(ref o) => o.uuid().clone(),
            Wrapper::Master(ref o) => o.uuid().clone(),
            Wrapper::Volume(ref o) => o.uuid().clone(),
            Wrapper::None => None,
        }
    }

    /// Extract the parent from the wrapper
    pub fn parent_uuid(&self) -> Option<String> {
        match *self {
            Wrapper::Version(ref o) => o.parent().clone(),
            Wrapper::Master(ref o) => o.parent().clone(),
            Wrapper::Volume(ref o) => o.parent().clone(),
            Wrapper::None => None,
        }
    }
}
