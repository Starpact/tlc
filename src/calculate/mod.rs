mod test;

use ffmpeg::format::{input, Pixel};
use ffmpeg::software::scaling::{context::Context, flag::Flags};
use ffmpeg::util::frame::video::Video;
use ffmpeg_next as ffmpeg;

use ndarray::parallel::prelude::*;
use ndarray::prelude::*;

/// *read the video and collect all green values spatially and temporally*
/// ### Argument:
/// video record(start frame, frame num, video path)
///
/// region record((upper left y, upper left x), (calculate region height, calculate region width))
/// ### Return:
/// (green values 2D matrix, frame rate)
///
/// * pixels in rows, frames in columns, shape: (total_pix_num, frame_num)
/// ### Paincs
/// ffmpeg errors
pub fn read_video(
    video_record: (usize, usize, &String),
    region_record: ((usize, usize), (usize, usize)),
) -> Result<(Array2<u8>, usize), ffmpeg::Error> {
    ffmpeg::init().expect("ffmpeg failed to initialize");

    let (start_frame, frame_num, video_path) = video_record;
    let mut ictx = input(video_path)?;
    let mut decoder = ictx.stream(0).unwrap().codec().decoder().video()?;

    let rational = decoder.frame_rate().unwrap();
    let frame_rate = (rational.numerator() / rational.denominator()) as usize;
    let total_frame = ictx.duration() as usize * frame_rate / 1_000_000;

    if start_frame + frame_num >= total_frame {
        return Err(ffmpeg::Error::InvalidData);
    }

    // upper_left_coordinate
    let (ul_y, ul_x) = region_record.0;
    // height and width of calculation region
    let (cal_h, cal_w) = region_record.1;
    // total number of pixels in the calculation region
    let pix_num = cal_h * cal_w;

    // Target color space: RGB24, 8 bits respectively for R, G and B
    let mut scaler = Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGB24,
        decoder.width(),
        decoder.height(),
        Flags::FAST_BILINEAR,
    )?;

    // g2d stores green values of all pixels at all frames in a 2D array: single row for all pixels at single frame
    let mut g2d = Array2::zeros((frame_num, pix_num));
    let real_w = decoder.width() as usize * 3;

    for (frame_index, (_, packet)) in (0..frame_num).zip(ictx.packets().skip(start_frame)) {
        decoder.send_packet(&packet)?;
        let (mut raw_frame, mut rgb_frame) = (Video::empty(), Video::empty());
        decoder.receive_frame(&mut raw_frame)?;
        scaler.run(&raw_frame, &mut rgb_frame)?;
        // the data of each frame stores in one 1D array: rgb|rbg|rgb|...|rgb, and row_0|row_1|...|row_n
        let rgb = rgb_frame.data(0);

        let mut row = g2d.row_mut(frame_index);
        let mut iter = row.iter_mut();
        for i in (0..).step_by(real_w).skip(ul_y).take(cal_h) {
            for j in (i..).skip(1).step_by(3).skip(ul_x).take(cal_w) {
                *(iter.next().unwrap()) = rgb[j];
            }
        }
    }

    Ok((g2d, frame_rate))
}

/// *traverse along the timeline to detect the peak of green values and record that frame index*
/// ### Argument:
/// green values 2D matrix
/// ### Return:
/// frame indexes of maximal green values
pub fn detect_peak(g2d: Array2<u8>) -> Array1<usize> {
    let mut peak_frames = Vec::with_capacity(g2d.ncols());

    g2d.axis_iter(Axis(1))
        .into_par_iter()
        .map(|column| column.iter().enumerate().max_by_key(|x| x.1).unwrap().0)
        .collect_into_vec(&mut peak_frames);

    Array1::from(peak_frames)
}
