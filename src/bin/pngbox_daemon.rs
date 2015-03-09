// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![crate_name = "pngbox_daemon"]
#![crate_type = "bin"]

#![feature(libc)]

extern crate libc;
extern crate urpc;
extern crate gaol;
extern crate pngbox;
extern crate unix_socket;

use std::env;
use unix_socket::UnixStream;
use gaol::sandbox::{ChildSandbox, ChildSandboxMethods};
use pngbox::SandboxedDecoder;

pub fn main() {
    let stream = UnixStream::from_fd(env::args()
        .skip(1).next().unwrap()
        .parse().unwrap());

    let profile = SandboxedDecoder::profile();
    ChildSandbox::new(profile).activate().unwrap();

    pngbox::png::serve(pngbox::LocalDecoder, stream).unwrap();
}
