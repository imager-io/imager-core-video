#![allow(unused)]
pub mod vmaf;
pub mod encoder;
pub mod yuv420p;

fn main() {
    // encoder::run();
    vmaf::run();
}
