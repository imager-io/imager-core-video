// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
use std::convert::AsRef;
use std::path::{PathBuf, Path};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use libc::{size_t, c_float, c_void, fread};
use x264_dev::{raw, sys};
use crate::yuv420p::Yuv420P;

///////////////////////////////////////////////////////////////////////////////
// DATA TYPES
///////////////////////////////////////////////////////////////////////////////

// #[derive(Clone)]
// pub struct Yuv420pImage {
//     pub width: u32,
//     pub height: u32,
//     pub linesize: u32,
//     pub buffer: Vec<u8>,
// }

// impl Yuv420pImage {
//     pub fn open<P: AsRef<Path>>(path: P) -> Self {
//         let media = ::image::open(path).expect("failed to read image file");
//         Yuv420pImage::from_image(&media)
//     }
//     pub fn decode_with_format(data: &Vec<u8>, format: ::image::ImageFormat) -> Self {
//         use image::{DynamicImage, GenericImage, GenericImageView};
//         let data = ::image::load_from_memory_with_format(data, format).expect("load image from memory");
//         Yuv420pImage::from_image(&data)
//     }
//     pub fn from_image(data: &::image::DynamicImage) -> Self {
//         use image::{DynamicImage, GenericImage, GenericImageView};
//         let (width, height) = data.dimensions();
//         let rgb = data
//             .to_rgb()
//             .pixels()
//             .map(|x| x.0.to_vec())
//             .flatten()
//             .collect::<Vec<_>>();
//         let yuv = rgb2yuv420::convert_rgb_to_yuv420p(&rgb, width, height, 3);
//         Yuv420pImage {
//             width,
//             height,
//             linesize: width,
//             buffer: yuv,
//         }
//     }
// }

///////////////////////////////////////////////////////////////////////////////
// FUNCTIONS
///////////////////////////////////////////////////////////////////////////////

fn init_param(width: u32, height: u32) -> sys::X264ParamT {
    let mut param: sys::X264ParamT = unsafe {std::mem::zeroed()};
    // let preset = CString::new("placebo").expect("CString failed");
    let preset = CString::new("medium").expect("CString failed");
    let tune = CString::new("ssim").expect("CString failed");
    let profile = CString::new("high").expect("CString failed");
    
    unsafe {
        let status = sys::x264_param_default_preset(
            &mut param,
            preset.as_ptr(),
            tune.as_ptr(),
        );
        assert!(status == 0);
    };

    param.i_bitdepth = 8;
    param.i_csp = raw::X264_CSP_I420 as i32;
    param.i_width  = width as i32;
    param.i_height = height as i32;
    param.b_vfr_input = 0;
    param.b_repeat_headers = 1;
    param.b_annexb = 1;

    unsafe {
        let status = sys::x264_param_apply_profile(&mut param, profile.as_ptr());
        assert!(status == 0);
    };

    param
}

fn encode() {
    // SOURCE
    let mut source = Yuv420P::open("assets/samples/2yV-pyOxnPw300.jpeg");
    // SETUP
    let (width, height) = (source.width, source.height);
    let linesize = width;
    let luma_size = width * height;
    let chroma_size = luma_size / 4;
    let mut param = init_param(width, height);
    let mut picture: sys::X264PictureT = unsafe {std::mem::zeroed()};
    let mut picture_output: sys::X264PictureT = unsafe {std::mem::zeroed()};
    unsafe {
        let status = sys::x264_picture_alloc(
            &mut picture,
            param.i_csp,
            param.i_width,
            param.i_height
        );
        assert!(status == 0);
    };
    let mut encoder: *mut sys::X264T = unsafe {
        sys::x264_encoder_open(&mut param)
    };
    assert!(!encoder.is_null());
    assert!(picture.img.i_plane == 3);
    assert!(picture.img.i_stride[0] == width as i32);
    assert!(picture.img.i_stride[1] == (width / 2) as i32);
    assert!(picture.img.i_stride[2] == (width / 2) as i32);
    // ???
    let mut p_nal: *mut sys::X264NalT = std::ptr::null_mut();
    let mut i_nal: i32 = unsafe { std::mem::zeroed() };
    // ENCODED OUTPUT
    let mut output = Vec::<u8>::new();
    // GO!
    for frame in 0..1 {
        let (mut y_ptr, mut u_ptr, mut v_ptr) = unsafe {(
            std::slice::from_raw_parts_mut(picture.img.plane[0], luma_size as usize),
            std::slice::from_raw_parts_mut(picture.img.plane[1], chroma_size as usize),
            std::slice::from_raw_parts_mut(picture.img.plane[2], chroma_size as usize),
        )};
        y_ptr.copy_from_slice(&source.y);
        u_ptr.copy_from_slice(&source.u);
        v_ptr.copy_from_slice(&source.v);
        // META
        picture.i_pts = frame;
        // ENCODE
        let i_frame_size = unsafe {
            sys::x264_encoder_encode(
                encoder,
                &mut p_nal,
                &mut i_nal,
                &mut picture,
                &mut picture_output,
            )
        };
        assert!(i_frame_size >= 0);
        if i_frame_size > 0 {
            let encoded = unsafe {
                std::slice::from_raw_parts(
                    (*p_nal).p_payload,
                    i_frame_size as usize,
                )
            };
            output.extend_from_slice(encoded);
        }
    }
    // FLUSH DELAYED FRAMES
    while unsafe{ sys::x264_encoder_delayed_frames(encoder) > 0 } {
        let i_frame_size = unsafe {
            sys::x264_encoder_encode(
                encoder,
                &mut p_nal,
                &mut i_nal,
                std::ptr::null_mut(),
                &mut picture_output,
            )
        };
        assert!(i_frame_size >= 0);
        if i_frame_size > 0 {
            let encoded = unsafe {
                std::slice::from_raw_parts(
                    (*p_nal).p_payload,
                    i_frame_size as usize,
                )
            };
            output.extend_from_slice(encoded);
        }
    }
    // CLEANUP
    unsafe {
        sys::x264_encoder_close(encoder);
        sys::x264_picture_clean(&mut picture);
    };
    // DONE
    std::fs::write("assets/output/test.h264", &output);
}

///////////////////////////////////////////////////////////////////////////////
// DEV
///////////////////////////////////////////////////////////////////////////////

pub fn run() {
    encode();
}