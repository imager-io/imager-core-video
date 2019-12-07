// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
use std::path::PathBuf;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use libc::{size_t, c_float, c_void};
use crate::yuv420p::Yuv420P;

///////////////////////////////////////////////////////////////////////////////
// MEDIA
///////////////////////////////////////////////////////////////////////////////

// #[derive(Clone)]
// pub struct Yuv420pImage {
//     pub width: u32,
//     pub height: u32,
//     pub linesize: u32,
//     pub buffer: Vec<u8>,
// }

// impl Yuv420pImage {
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
// VMAF CONTEXT
///////////////////////////////////////////////////////////////////////////////

struct Context {
    width: u32,
    height: u32,
    frames: Vec<PathBuf>,
    frames_set: bool,
}

#[repr(C)]
struct VmafReportContext {
    source1: Yuv420P,
    source2: Yuv420P,
    frames_set: bool,
}


///////////////////////////////////////////////////////////////////////////////
// VMAF CALLBACK
///////////////////////////////////////////////////////////////////////////////


unsafe fn fill_vmaf_buffer(
    mut output: *mut c_float,
    output_stride: c_int,
    source: &Yuv420P,
) {
    for (i, px) in source.y.iter().enumerate() {
        *output.offset(i as isize) = px.clone() as c_float;
    }
}

unsafe extern "C" fn read_frame(
    mut source1_out: *mut c_float,
    mut source2_out: *mut c_float,
    temp_data: *mut c_float,
    out_stride: c_int,
    raw_ctx: *mut libc::c_void,
) -> c_int {
    // CONTEXT
    let mut vmaf_ctx = Box::from_raw(raw_ctx as *mut VmafReportContext);
    let mut vmaf_ctx = Box::leak(vmaf_ctx);

    // DONE
    if vmaf_ctx.frames_set {
        return 2;
    }
    
    // FILL BUFFERS (THE EXTREMELY UNSAFE, DANGEROUS AND CONFUSING PART)
    fill_vmaf_buffer(source1_out, out_stride, &vmaf_ctx.source1);
    fill_vmaf_buffer(source2_out, out_stride, &vmaf_ctx.source2);
    vmaf_ctx.frames_set = true;
    return 0;
}


///////////////////////////////////////////////////////////////////////////////
// VMAF PIPELINE
///////////////////////////////////////////////////////////////////////////////

unsafe fn vmaf_controller(source1: Yuv420P, source2: Yuv420P) -> f64 {
    // RESOLUTION REQUIREMENTS
    assert!(source1.width == source2.width);
    assert!(source1.height == source2.height);

    // INIT VMAF CONTEXT
    let mut vmaf_ctx = Box::new(VmafReportContext {
        source1: source1.clone(),
        source2: source2.clone(),
        frames_set: false
    });
    let vmaf_ctx = Box::into_raw(vmaf_ctx);

    // SETTINGS
    let mut vmaf_score = 0.0;
    let model_path = vmaf_sys::extras::get_4k_model_path()
        .to_str()
        .expect("PathBuf to str failed")
        .to_owned();
    let model_path = CString::new(model_path).expect("CString::new failed");
    let mut fmt = CString::new(String::from("yuv420p")).expect("CString::new failed");
    let width = source1.width;
    let height = source1.height;
    let log_path: *mut c_char = std::ptr::null_mut();
    let log_fmt: *mut c_char = std::ptr::null_mut();
    let disable_clip = 0;
    let disable_avx = 0;
    let enable_transform = 0;
    let phone_model = 0;
    let do_psnr = 0;
    let do_ssim = 0;
    let do_ms_ssim = 0;
    let pool_method: *mut c_char = std::ptr::null_mut();
    let n_thread = 1;
    let n_subsample = 1;
    let enable_conf_interval = 0;

    // GO!
    let compute_vmaf_res = vmaf_sys::compute_vmaf(
        &mut vmaf_score,
        fmt.as_ptr() as *mut c_char,
        width as c_int,
        height as c_int,
        Some(read_frame),
        vmaf_ctx as *mut libc::c_void,
        model_path.as_ptr() as *mut c_char,
        log_path,
        log_fmt,
        disable_clip,
        disable_avx,
        enable_transform,
        phone_model,
        do_psnr,
        do_ssim,
        do_ms_ssim,
        pool_method,
        n_thread,
        n_subsample,
        enable_conf_interval
    );

    // CHECK
    assert!(compute_vmaf_res == 0);

    // CLEANUP
    let mut vmaf_ctx = Box::from_raw(vmaf_ctx);
    std::mem::drop(vmaf_ctx);

    // DONE
    vmaf_score
}
