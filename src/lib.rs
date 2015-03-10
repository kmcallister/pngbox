// Copyright 2013-2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![crate_name = "pngbox"]
#![crate_type = "lib"]

#![allow(unused_features)]
#![feature(core, libc, test, std_misc, old_io, old_path)]

extern crate libc;
extern crate "rustc-serialize" as rustc_serialize;
extern crate unix_socket;
extern crate gaol;
extern crate "test" as rust_test;

#[macro_use]
extern crate urpc;

use libc::{c_int, size_t};
use std::{mem, ptr, slice};
use std::ops::{Deref, DerefMut};
use std::iter::repeat;
use std::os::unix::AsRawFd;
use unix_socket::UnixStream;
use gaol::profile::Profile;
use gaol::sandbox::{Sandbox, SandboxMethods, Command};

mod ffi;

#[derive(PartialEq, Eq, RustcEncodable, RustcDecodable)]
pub enum PixelsByColorType {
    K8(Vec<u8>),
    KA8(Vec<u8>),
    RGB8(Vec<u8>),
    RGBA8(Vec<u8>),
}

#[derive(PartialEq, Eq, RustcEncodable, RustcDecodable)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: PixelsByColorType,
}

#[derive(PartialEq, Eq, RustcEncodable, RustcDecodable)]
pub enum DecodeResult {
    Image(Image),
    Error(String),
}

urpc! {
    pub interface png {
        fn decode(compressed: Vec<u8>) -> ::DecodeResult { }
    }
}

pub struct LocalDecoder;

impl png::Methods for LocalDecoder {
    fn decode(&mut self, compressed: Vec<u8>) -> urpc::Result<DecodeResult> {
        Ok(decode_from_memory(&compressed))
    }
}

pub struct SandboxedDecoder(png::Client<UnixStream>);

impl Deref for SandboxedDecoder {
    type Target = png::Client<UnixStream>;
    fn deref(&self) -> &png::Client<UnixStream> { &self.0 }
}

impl DerefMut for SandboxedDecoder {
    fn deref_mut(&mut self) -> &mut png::Client<UnixStream> { &mut self.0 }
}

impl SandboxedDecoder {
    pub fn profile() -> Profile {
        // No operations allowed.
        Profile::new(vec![]).unwrap()
    }

    pub fn new() -> SandboxedDecoder {
        let [s1, s2] = UnixStream::unnamed().unwrap();
        let mut command = Command::new(b"/home/keegan/pngbox/target/pngbox_daemon");
        command.arg(&format!("{}", s1.as_raw_fd()));
        let profile = SandboxedDecoder::profile();
        Sandbox::new(profile).start(&mut command).unwrap();

        SandboxedDecoder(png::Client::new(s2))
    }
}

// This intermediate data structure is used to read
// an image data from 'offset' position, and store it
// to the data vector.
struct ImageData<'a> {
    data: &'a [u8],
    offset: usize,
}

extern "C" fn read_data(png_ptr: *mut ffi::png_struct, data: *mut u8, length: size_t) {
    unsafe {
        let io_ptr = ffi::RUST_png_get_io_ptr(png_ptr);
        let image_data: &mut ImageData = mem::transmute(io_ptr);
        let len = length as usize;
        let buf = slice::from_raw_parts_mut(data, len);
        let end_pos = std::cmp::min(image_data.data.len()-image_data.offset, len);
        let src = &image_data.data[image_data.offset..image_data.offset+end_pos];

        ptr::copy(buf.as_mut_ptr(), src.as_ptr(), src.len());
        image_data.offset += end_pos;
    }
}

fn decode_from_memory(image: &[u8]) -> DecodeResult {
    unsafe {
        let mut png_ptr = ffi::RUST_png_create_read_struct(&*ffi::RUST_png_get_header_ver(ptr::null_mut()),
                                                      ptr::null_mut(),
                                                      ptr::null_mut(),
                                                      ptr::null_mut());
        if png_ptr.is_null() {
            return DecodeResult::Error("could not create read struct".to_string());
        }
        let mut info_ptr = ffi::RUST_png_create_info_struct(png_ptr);
        if info_ptr.is_null() {
            ffi::RUST_png_destroy_read_struct(&mut png_ptr, ptr::null_mut(), ptr::null_mut());
            return DecodeResult::Error("could not create info struct".to_string());
        }
        let res = ffi::setjmp(ffi::pngshim_jmpbuf(png_ptr));
        if res != 0 {
            ffi::RUST_png_destroy_read_struct(&mut png_ptr, &mut info_ptr, ptr::null_mut());
            return DecodeResult::Error("error reading png".to_string());
        }

        let mut image_data = ImageData {
            data: image,
            offset: 0,
        };

        ffi::RUST_png_set_read_fn(png_ptr, mem::transmute(&mut image_data), read_data);
        ffi::RUST_png_read_info(png_ptr, info_ptr);

        let width = ffi::RUST_png_get_image_width(png_ptr, info_ptr) as usize;
        let height = ffi::RUST_png_get_image_height(png_ptr, info_ptr) as usize;
        let color_type = ffi::RUST_png_get_color_type(png_ptr, info_ptr);
        let bit_depth = ffi::RUST_png_get_bit_depth(png_ptr, info_ptr);

        // convert palette and grayscale to rgb
        match color_type as c_int {
            ffi::COLOR_TYPE_PALETTE => {
                ffi::RUST_png_set_palette_to_rgb(png_ptr);
            }
            ffi::COLOR_TYPE_GRAY | ffi::COLOR_TYPE_GRAY_ALPHA => {
                ffi::RUST_png_set_gray_to_rgb(png_ptr);
            }
            _ => {}
        }

        // convert 16-bit channels to 8-bit
        if bit_depth == 16 {
            ffi::RUST_png_set_strip_16(png_ptr);
        }

        // add alpha channels
        ffi::RUST_png_set_add_alpha(png_ptr, 0xff, ffi::FILLER_AFTER);
        if ffi::RUST_png_get_valid(png_ptr, info_ptr, ffi::INFO_tRNS as u32) != 0 {
            ffi::RUST_png_set_tRNS_to_alpha(png_ptr);
        }

        ffi::RUST_png_set_packing(png_ptr);
        ffi::RUST_png_set_interlace_handling(png_ptr);
        ffi::RUST_png_read_update_info(png_ptr, info_ptr);

        let updated_bit_depth = ffi::RUST_png_get_bit_depth(png_ptr, info_ptr);
        let updated_color_type = ffi::RUST_png_get_color_type(png_ptr, info_ptr);

        let (color_type, pixel_width) = match (updated_color_type as c_int, updated_bit_depth) {
            (ffi::COLOR_TYPE_RGB, 8) |
            (ffi::COLOR_TYPE_RGBA, 8) |
            (ffi::COLOR_TYPE_PALETTE, 8) => (PixelsByColorType::RGBA8 as fn(Vec<u8>) -> PixelsByColorType, 4usize),
            (ffi::COLOR_TYPE_GRAY, 8) => (PixelsByColorType::K8 as fn(Vec<u8>) -> PixelsByColorType, 1usize),
            (ffi::COLOR_TYPE_GA, 8) => (PixelsByColorType::KA8 as fn(Vec<u8>) -> PixelsByColorType, 2usize),
            _ => panic!("color type not supported"),
        };

        let mut image_data: Vec<u8> = repeat(0u8).take(width * height * pixel_width).collect();
        let image_buf = image_data.as_mut_ptr();
        let mut row_pointers: Vec<*mut u8> = (0..height).map(|idx| {
            image_buf.offset((width * pixel_width * idx) as isize)
        }).collect();

        ffi::RUST_png_read_image(png_ptr, row_pointers.as_mut_ptr());

        ffi::RUST_png_destroy_read_struct(&mut png_ptr, &mut info_ptr, ptr::null_mut());

        DecodeResult::Image(Image {
            width: width as u32,
            height: height as u32,
            pixels: color_type(image_data),
        })
    }
}

#[cfg(test)]
mod test {
    use std::old_io::File;
    use super::{DecodeResult, LocalDecoder, SandboxedDecoder};
    use super::PixelsByColorType::RGBA8;
    use rust_test::Bencher;

    fn load_rgba8<D>(decoder: &mut D, file: &'static str, w: u32, h: u32) -> Vec<u8>
        where D: super::png::Methods,
    {
        let contents = File::open(&Path::new(file)).read_to_end().unwrap();
        match decoder.decode(contents).unwrap() {
            DecodeResult::Error(m) => panic!(m),
            DecodeResult::Image(image) => {
                assert_eq!(image.width, w);
                assert_eq!(image.height, h);
                match image.pixels {
                    RGBA8(px) => px,
                    _ => panic!("Expected RGBA8")
                }
            }
        }
    }

    #[test]
    fn test_sandboxed() {
        let mut d = SandboxedDecoder::new();

        assert_eq!(load_rgba8(&mut *d, "test/servo-screenshot.png", 831, 624),
            load_rgba8(&mut LocalDecoder, "test/servo-screenshot.png", 831, 624));

        assert_eq!(load_rgba8(&mut *d, "test/gray.png", 100, 100),
            load_rgba8(&mut LocalDecoder, "test/gray.png", 100, 100));
    }

    #[bench]
    fn bench_local(bh: &mut Bencher) {
        use super::png::Methods;

        let contents = File::open(&Path::new("test/servo-screenshot.png"))
            .read_to_end().unwrap();
        bh.iter(|| {
            assert!(LocalDecoder.decode(contents.clone()).is_ok());
        });
    }

    #[bench]
    fn bench_sandboxed(bh: &mut Bencher) {
        use super::png::Methods;

        let mut decoder = SandboxedDecoder::new();
        let contents = File::open(&Path::new("test/servo-screenshot.png"))
            .read_to_end().unwrap();
        bh.iter(|| {
            assert!(decoder.decode(contents.clone()).is_ok());
        });
    }
}
