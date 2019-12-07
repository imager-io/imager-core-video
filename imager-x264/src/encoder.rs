// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
use std::convert::AsRef;
use std::path::{PathBuf, Path};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::collections::VecDeque;
use libc::{size_t, c_float, c_void, fread};
use x264_dev::{raw, sys};
use itertools::Itertools;
use crate::yuv420p::Yuv420P;
use crate::stream::{Stream, FileStream, SingleImage};


///////////////////////////////////////////////////////////////////////////////
// FUNCTIONS
///////////////////////////////////////////////////////////////////////////////

fn init_param(width: u32, height: u32, dump: &str) -> sys::X264ParamT {
    let mut param: sys::X264ParamT = unsafe {std::mem::zeroed()};
    // let preset = CString::new("placebo").expect("CString failed");
    let preset = CString::new("ultrafast").expect("CString failed");
    // let preset = CString::new("medium").expect("CString failed");
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

    param.rc.i_rc_method = 1;
    // param.rc.i_qp_constant = 10;
    param.rc.f_rf_constant = 40.0;
    param.i_bitdepth = 8;
    param.i_csp = raw::X264_CSP_I420 as i32;
    param.i_width  = width as i32;
    param.i_height = height as i32;
    param.b_vfr_input = 0;
    param.b_repeat_headers = 1;
    param.b_annexb = 1;
    param.b_full_recon = 1;

    unsafe {
        let status = sys::x264_param_apply_profile(&mut param, profile.as_ptr());
        assert!(status == 0);
    };

    param
}

fn encode() {
    // SOURCE
    let source_path = "assets/samples/sintel-trailer-gop1";
    // let source_path = "assets/samples/gop-test-single";
    let mut stream = FileStream::new(source_path, 1920, 818);
    let mut vmaf_source = SingleImage::new(stream.width, stream.height);
    let mut vmaf_derivative = SingleImage::new(stream.width, stream.height);
    // SETUP
    // let yuv_dump = CString::new("")
    let (width, height) = (stream.width, stream.height);
    let linesize = width;
    let luma_size = width * height;
    let chroma_size = luma_size / 4;
    let mut param = init_param(width, height, "assets/output/dump");
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
    while let Some(source) = stream.next() {
        let (mut y_ptr, mut u_ptr, mut v_ptr) = unsafe {(
            std::slice::from_raw_parts_mut(picture.img.plane[0], luma_size as usize),
            std::slice::from_raw_parts_mut(picture.img.plane[1], chroma_size as usize),
            std::slice::from_raw_parts_mut(picture.img.plane[2], chroma_size as usize),
        )};
        y_ptr.copy_from_slice(&source.y);
        u_ptr.copy_from_slice(&source.u);
        v_ptr.copy_from_slice(&source.v);
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
        // RECONSTRUCT - GET DECODED DERIVATIVE
        let report = unsafe {
            vmaf_source.yuv = source;
            vmaf_source.restart();
            let (mut y_ptr, mut u_ptr, mut v_ptr) = unsafe {(
                std::slice::from_raw_parts_mut(picture.img.plane[0], luma_size as usize),
                std::slice::from_raw_parts_mut(picture.img.plane[1], chroma_size as usize),
                std::slice::from_raw_parts_mut(picture.img.plane[2], chroma_size as usize),
            )};
            vmaf_derivative.yuv.y.copy_from_slice(y_ptr);
            vmaf_derivative.yuv.u.copy_from_slice(u_ptr);
            vmaf_derivative.yuv.v.copy_from_slice(v_ptr);
            vmaf_derivative.restart();
            crate::vmaf::vmaf_controller(
                Box::new(vmaf_source.clone()),
                Box::new(vmaf_derivative.clone()),
            )
        };
        println!("report: {:?}", report);
        // if picture_output.img.plane[0].is_null() {
        //     println!("NULL");
        // } else {
        //     println!("NOT NULL");
        // }
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