use std::cell::Ref;
use std::fs::{create_dir_all, File};
use std::io::{BufReader, Read};
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::Mutex;
use std::{cell::RefCell, io::BufWriter};

use calamine::{open_workbook, Reader, Xlsx};
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use ffmpeg::software::scaling::flag::Flags;
use ffmpeg::util::frame::video::Video;
use ffmpeg::{codec::Context, media::Type};
use ffmpeg::{format, Packet};
use ffmpeg_next as ffmpeg;
use ndarray::parallel::prelude::*;
use ndarray::prelude::*;
use serde_json::{from_reader, to_writer_pretty};
use thread_local::ThreadLocal;

use super::{error::TLCResult, postprocess, DEFAULT_CONFIG_PATH};
use super::{TLCConfig, TLCData, Thermocouple};
use crate::awsl;

/// 视频帧压缩后发送给前端
const COMPRESSION_RATIO: u32 = 2;

/// wrap `Context` to pass between threads(because of the raw pointer)
struct SendCtx(ffmpeg::software::scaling::Context);

unsafe impl Send for SendCtx {}

impl Deref for SendCtx {
    type Target = ffmpeg::software::scaling::Context;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SendCtx {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct VideoCtx(Mutex<Context>);

impl Deref for VideoCtx {
    type Target = Mutex<Context>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct Decoder {
    decoder: RefCell<ffmpeg::decoder::Video>,
    sws_ctx: RefCell<SendCtx>,
    src_frame: RefCell<Video>,
    dst_frame: RefCell<Video>,
}

impl Decoder {
    fn new(video_ctx: &VideoCtx, compress: bool) -> TLCResult<Self> {
        let decoder = video_ctx
            .lock()
            .map_err(|err| awsl!(err))?
            .clone()
            .decoder()
            .video()
            .map_err(|err| awsl!(VideoError, err, ""))?;
        let (h, w) = (decoder.height(), decoder.width());
        let sws_ctx = ffmpeg::software::scaling::Context::get(
            decoder.format(),
            w,
            h,
            format::Pixel::RGB24,
            if compress { w / COMPRESSION_RATIO } else { w },
            if compress { h / COMPRESSION_RATIO } else { h },
            Flags::FAST_BILINEAR,
        )
        .map_err(|err| awsl!(VideoError, err, ""))?;

        Ok(Self {
            decoder: RefCell::new(decoder),
            sws_ctx: RefCell::new(SendCtx(sws_ctx)),
            src_frame: RefCell::new(Video::empty()),
            dst_frame: RefCell::new(Video::empty()),
        })
    }

    fn decode(&self, packet: &Packet) -> TLCResult<Ref<Video>> {
        let mut decoder = self.decoder.borrow_mut();
        let mut sws_ctx = self.sws_ctx.borrow_mut();
        let mut src_frame = self.src_frame.borrow_mut();
        let mut dst_frame = self.dst_frame.borrow_mut();

        decoder
            .send_packet(packet)
            .map_err(|err| awsl!(VideoError, err, "发送数据包错误"))?;
        decoder
            .receive_frame(&mut src_frame)
            .map_err(|err| awsl!(VideoError, err, "接受数据帧错误"))?;
        sws_ctx
            .run(&src_frame, &mut dst_frame)
            .map_err(|err| awsl!(VideoError, err, "颜色转换错误"))?;
        drop(dst_frame);

        Ok(self.dst_frame.borrow())
    }
}

impl TLCData {
    pub fn get_frame(&mut self, frame_index: usize) -> TLCResult<String> {
        if self.video_ctx.is_none() {
            self.video_ctx.insert(self.create_video_ctx()?);
        }
        if self.decoder_tool.is_none() {
            self.decoder_tool
                .insert(Decoder::new(self.get_video_ctx()?, true)?);
        }

        let packets = loop {
            let packets = self.packets.lock().map_err(|err| awsl!(err))?;
            if frame_index < packets.len() {
                break packets;
            }
        };

        let decoder = self.get_decoder()?;
        let dst_frame = decoder.decode(&packets[frame_index])?;

        let (src_h, src_w) = self.get_config().video_shape;
        let dst_h = src_h as u32 / COMPRESSION_RATIO;
        let dst_w = src_w as u32 / COMPRESSION_RATIO;
        let mut buf = Vec::with_capacity((dst_h * dst_w >> 2) as usize);

        let mut jpeg_encoder = image::jpeg::JpegEncoder::new(&mut buf);
        jpeg_encoder
            .encode(dst_frame.data(0), dst_w, dst_h, image::ColorType::Rgb8)
            .map_err(|err| awsl!(err))?;
        let base64_string = base64::encode(&buf);

        Ok(base64_string)
    }

    pub fn read_video(&mut self) -> TLCResult<&mut Self> {
        if self.video_ctx.is_none() {
            self.video_ctx.insert(self.create_video_ctx()?);
        }

        let TLCConfig {
            top_left_pos,
            region_shape,
            start_frame,
            frame_num,
            video_shape,
            ..
        } = self.config;

        // 左上角坐标
        let (tl_y, tl_x) = top_left_pos;
        // 区域尺寸
        let (cal_h, cal_w) = region_shape;
        // 总像素点数
        let pix_num = cal_h * cal_w;
        // 视频帧一行实际字节数
        let real_w = (video_shape.1 * 3) as usize;

        let ctx_mutex = self.get_video_ctx()?;
        let mut g2d = Array2::zeros((frame_num, pix_num));
        let tls = ThreadLocal::new();
        let tls_ref = &tls;
        let packets = loop {
            let packets = self.packets.lock().map_err(|err| awsl!(err))?;
            if packets.len() == self.config.total_frames {
                break packets;
            }
        };

        packets
            .par_iter()
            .skip(start_frame)
            .zip(g2d.axis_iter_mut(Axis(0)).into_par_iter())
            .try_for_each(|(packet, mut row)| -> TLCResult<()> {
                let dst_frame = tls_ref
                    .get_or_try(|| Decoder::new(ctx_mutex, false))?
                    .decode(packet)?;

                // the data of each frame store in one u8 array:
                // ||r g b r g b...r g b|......|r g b r g b...r g b||
                // ||.......row_0.......|......|.......row_n.......||
                let rgb = dst_frame.data(0);
                let mut it = row.iter_mut();

                for i in (0..).step_by(real_w).skip(tl_y).take(cal_h) {
                    for j in (i..).skip(1).step_by(3).skip(tl_x).take(cal_w) {
                        *it.next().unwrap() = unsafe { *rgb.get_unchecked(j) };
                    }
                }

                Ok(())
            })?;
        self.raw_g2d.insert(g2d);

        // 确保thread local析构
        tls.into_iter().for_each(|v| drop(v));
        drop(packets);
        self.drop_video();

        Ok(self)
    }

    pub fn read_daq(&mut self) -> TLCResult<&mut Self> {
        self.daq.insert(self.config.read_daq()?);

        Ok(self)
    }

    pub fn drop_video(&mut self) {
        // 缓存的视频数据包析构
        if let Ok(mut ps) = self.packets.lock() {
            ps.clear();
        }
        // 解码相关内存析构
        self.video_ctx.take();
        self.decoder_tool.take();
    }

    pub fn get_nu_img(&mut self, range: Option<(f32, f32)>) -> TLCResult<String> {
        let (vmin, vmax) = match range {
            Some(range) => range,
            None => {
                let nu_nan_mean = self.get_nu_nan_mean()?;
                (nu_nan_mean * 0.6, nu_nan_mean * 2.)
            }
        };
        postprocess::plot_area(&self.config.plots_path, self.get_nu2d()?, vmin, vmax)?;
        let mut buf = Vec::new();
        File::open(&self.config.plots_path)
            .map_err(|err| awsl!(err))?
            .read_to_end(&mut buf)
            .map_err(|err| awsl!(err))?;
        let base64_string = base64::encode(&buf);

        Ok(base64_string)
    }

    pub fn create_video_ctx(&self) -> TLCResult<VideoCtx> {
        ffmpeg::init().map_err(|err| awsl!(VideoError, err, "ffmpeg初始化错误，建议重装"))?;
        let video_path = &self.config.video_path;
        let mut input = format::input(video_path).map_err(|_| awsl!(VideoIOError, video_path))?;
        let video_stream = input.streams().best(Type::Video).ok_or(awsl!(
            VideoError,
            "找不到视频流",
            video_path,
        ))?;
        let video_stream_index = video_stream.index();
        let ctx_mutex = Mutex::new(video_stream.codec());
        let total_frames = self.config.total_frames;

        let packets = self.packets.clone();
        std::thread::spawn(move || -> TLCResult<()> {
            let mut ps = packets.lock().map_err(|err| awsl!(err))?;
            ps.clear();
            *ps = Vec::with_capacity(total_frames);
            drop(ps);
            let mut cnt = 0;
            for (stream, packet) in input.packets() {
                if stream.index() == video_stream_index {
                    packets.lock().map_err(|err| awsl!(err))?.push(packet);
                    cnt += 1;
                }
                if cnt == total_frames {
                    break;
                }
            }

            Ok(())
        });

        Ok(VideoCtx(ctx_mutex))
    }
}

impl TLCConfig {
    pub fn from_path<P: AsRef<Path>>(config_path: P) -> TLCResult<Self> {
        let file = File::open(config_path.as_ref())
            .map_err(|err| awsl!(ConfigIOError, err, config_path.as_ref()))?;
        let reader = BufReader::new(file);
        let mut cfg: TLCConfig = from_reader(reader).map_err(|err| awsl!(ConfigError, err))?;
        let _ = cfg.init_video_metadata();
        let _ = cfg.init_daq_metadata();
        let _ = cfg.init_path();
        if cfg.frame_num == 0 {
            cfg.init_frame_num();
        }
        cfg.init_regulator();

        Ok(cfg)
    }

    fn init_video_metadata(&mut self) -> TLCResult<&mut Self> {
        ffmpeg::init().map_err(|err| awsl!(VideoError, err, "ffmpeg初始化错误，建议重装"))?;

        let input =
            format::input(&self.video_path).map_err(|_| awsl!(VideoIOError, &self.video_path))?;
        let video_stream = input.streams().best(Type::Video).ok_or(awsl!(
            VideoError,
            "找不到视频流",
            &self.video_path,
        ))?;
        let rational = video_stream.avg_frame_rate();
        self.frame_rate =
            (rational.numerator() as f64 / rational.denominator() as f64).round() as usize;
        self.total_frames =
            (input.duration() as f64 * self.frame_rate as f64 / 1_000_000 as f64).floor() as usize;

        let decoder = video_stream
            .codec()
            .decoder()
            .video()
            .map_err(|err| awsl!(VideoError, err, ""))?;
        if self.video_shape == (0, 0) {
            self.video_shape = (decoder.height() as usize, decoder.width() as usize);
        }

        Ok(self)
    }

    fn init_daq_metadata(&mut self) -> TLCResult<&mut Self> {
        let daq_path = Path::new(&self.daq_path);
        self.total_rows = match daq_path
            .extension()
            .ok_or(awsl!(DAQIOError, "路径有误", daq_path))?
            .to_str()
            .ok_or(awsl!(DAQIOError, "路径有误", daq_path))?
        {
            "lvm" => ReaderBuilder::new()
                .has_headers(false)
                .from_path(daq_path)
                .map_err(|err| awsl!(DAQIOError, err, daq_path))?
                .records()
                .count(),
            "xlsx" => {
                let mut excel: Xlsx<_> =
                    open_workbook(daq_path).map_err(|err| awsl!(DAQIOError, err, daq_path))?;
                excel
                    .worksheet_range_at(0)
                    .ok_or(awsl!(DAQError, "找不到worksheet", daq_path))?
                    .map_err(|err| awsl!(DAQError, err, daq_path))?
                    .height()
            }
            _ => Err(awsl!(DAQIOError, "只支持.lvm或.xlsx格式", daq_path))?,
        };

        Ok(self)
    }

    fn init_frame_num(&mut self) -> &mut Self {
        self.frame_num =
            (self.total_frames - self.start_frame).min(self.total_rows - self.start_row);

        self
    }

    fn init_path(&mut self) -> TLCResult<&mut Self> {
        if self.save_dir == "" {
            return Err(awsl!(HandleError, "未设置保存根目录"));
        }
        let save_dir = Path::new(&self.save_dir);
        let config_dir = save_dir.join("config");
        let data_dir = save_dir.join("data");
        let plots_dir = save_dir.join("plots");

        create_dir_all(&config_dir).map_err(|err| awsl!(CreateDirError, err, config_dir))?;
        create_dir_all(&data_dir).map_err(|err| awsl!(CreateDirError, err, data_dir))?;
        create_dir_all(&plots_dir).map_err(|err| awsl!(CreateDirError, err, plots_dir))?;

        if self.video_path == "video_path" {
            return Ok(self);
        }

        self.case_name = Path::new(&self.video_path)
            .file_stem()
            .ok_or(awsl!(VideoIOError, &self.video_path))?
            .to_str()
            .ok_or(awsl!(VideoIOError, &self.video_path))?
            .to_owned();
        let config_path = config_dir.join(&self.case_name).with_extension("json");
        self.config_path = config_path.to_str().ok_or(awsl!(config_path))?.to_owned();
        let data_path = data_dir.join(&self.case_name).with_extension("csv");
        self.data_path = data_path.to_str().ok_or(awsl!(data_path))?.to_owned();
        let plots_path = plots_dir.join(&self.case_name).with_extension("png");
        self.plots_path = plots_path.to_str().ok_or(awsl!(plots_path))?.to_owned();

        Ok(self)
    }

    fn init_regulator(&mut self) -> &mut Self {
        if self.thermocouples.len() != self.regulator.len() {
            self.regulator = vec![1.; self.thermocouples.len()];
        }

        self
    }

    pub fn set_save_dir(&mut self, save_dir: String) -> TLCResult<&mut Self> {
        self.save_dir = save_dir;
        self.init_path()?;

        Ok(self)
    }

    pub fn set_video_path(&mut self, video_path: String) -> TLCResult<&mut Self> {
        self.video_path = video_path;
        self.init_video_metadata()?.init_frame_num().init_path()?;

        Ok(self)
    }

    pub fn set_daq_path(&mut self, daq_path: String) -> TLCResult<&mut Self> {
        self.daq_path = daq_path;
        self.init_daq_metadata()?.init_frame_num();

        Ok(self)
    }

    pub fn set_start_frame(&mut self, start_frame: usize) -> TLCResult<&mut Self> {
        if start_frame >= self.total_frames {
            return Err(awsl!(HandleError, "起始帧数超过视频总帧数"));
        }
        if self.start_row + start_frame < self.start_frame {
            return Err(awsl!(HandleError, "根据同步结果推算出的起始行数非正值"));
        }
        let start_row = self.start_row + start_frame - self.start_frame;
        if start_row >= self.total_rows {
            return Err(awsl!(
                HandleError,
                "根据同步结果推算出的起始行数超过数采文件总行数"
            ));
        }
        self.start_frame = start_frame;
        self.start_row = start_row;
        self.init_frame_num();

        Ok(self)
    }

    pub fn set_start_row(&mut self, start_row: usize) -> TLCResult<&mut Self> {
        if start_row >= self.total_rows {
            return Err(awsl!(HandleError, "起始行数超过数采文件总行数"));
        }
        if self.start_frame + start_row < self.start_row {
            return Err(awsl!(HandleError, "根据同步结果推算出的起始帧数非正值"));
        }
        let start_frame = self.start_frame + start_row - self.start_row;
        if start_frame >= self.total_frames {
            return Err(awsl!(
                HandleError,
                "根据同步结果推算出的起始帧数超过视频总帧数"
            ));
        }
        self.start_row = start_row;
        self.start_frame = start_frame;
        self.init_frame_num();

        Ok(self)
    }

    pub fn set_thermocouples(&mut self, thermocouples: Vec<Thermocouple>) -> &mut Self {
        self.thermocouples = thermocouples;
        self.init_regulator();

        self
    }

    pub fn synchronize(&mut self, frame_index: usize, row_index: usize) -> &mut Self {
        if frame_index < row_index {
            self.start_frame = 0;
            self.start_row = row_index - frame_index;
        } else {
            self.start_row = 0;
            self.start_frame = frame_index - row_index;
        }
        self.init_frame_num();

        self
    }

    /// 读取参考温度(.lvm or .xlsx)
    pub fn read_daq(&self) -> TLCResult<Array2<f32>> {
        let daq_path = Path::new(&self.daq_path);
        let daq = match daq_path
            .extension()
            .ok_or(awsl!(DAQIOError, "路径有误", daq_path))?
            .to_str()
            .ok_or(awsl!(DAQIOError, "路径有误", daq_path))?
        {
            "lvm" => self.read_daq_from_lvm(),
            "xlsx" => self.read_daq_from_excel(),
            _ => Err(awsl!(DAQIOError, "只支持.lvm或.xlsx格式", daq_path))?,
        }?;

        Ok(daq)
    }

    fn read_daq_from_lvm(&self) -> TLCResult<Array2<f32>> {
        let daq_path = Path::new(&self.daq_path);
        let total_columns = ReaderBuilder::new()
            .has_headers(false)
            .delimiter(b'\t')
            .from_path(daq_path)
            .map_err(|err| awsl!(DAQIOError, err, daq_path))?
            .records()
            .next()
            .ok_or(awsl!(DAQError, "数采文件为空", daq_path))?
            .map_err(|err| awsl!(DAQError, err, daq_path))?
            .len();

        let mut rdr = ReaderBuilder::new()
            .has_headers(false)
            .delimiter(b'\t')
            .from_path(daq_path)
            .map_err(|err| awsl!(DAQIOError, err, daq_path))?;

        let mut daq = Array2::zeros((self.total_rows, total_columns));
        for (csv_row_result, mut daq_column) in rdr.records().zip(daq.rows_mut()) {
            let csv_row = csv_row_result.map_err(|err| awsl!(DAQIOError, err, daq_path))?;
            for (csv_val, daq_val) in csv_row.into_iter().zip(daq_column.iter_mut()) {
                *daq_val = csv_val.parse::<f32>().map_err(|err| {
                    awsl!(
                        DAQError,
                        format!("数据采集文件中不应当有数字以外的格式{}", err),
                        daq_path,
                    )
                })?;
            }
        }

        Ok(daq)
    }

    fn read_daq_from_excel(&self) -> TLCResult<Array2<f32>> {
        let daq_path = Path::new(&self.daq_path);
        let mut excel: Xlsx<_> =
            open_workbook(daq_path).map_err(|err| awsl!(DAQIOError, err, daq_path))?;
        let sheet = excel
            .worksheet_range_at(0)
            .ok_or(awsl!(DAQError, "找不到worksheet", daq_path))?
            .map_err(|err| awsl!(DAQIOError, err, daq_path))?;
        let total_columns = sheet.width();

        let mut daq = Array2::zeros((self.total_rows, total_columns));
        for (excel_row, mut daq_col) in sheet.rows().zip(daq.rows_mut()) {
            for (excel_val, daq_val) in excel_row.into_iter().zip(daq_col.iter_mut()) {
                *daq_val = excel_val.get_float().ok_or(awsl!(
                    DAQError,
                    "数据采集文件中不应当有数字以外的格式",
                    daq_path,
                ))? as f32;
            }
        }

        Ok(daq)
    }

    /// 保存配置
    pub fn save(&self) -> TLCResult<()> {
        // 存到指定位置
        let file = File::create(&self.config_path)
            .map_err(|err| awsl!(ConfigIOError, err, self.config_path))?;
        let writer = BufWriter::new(file);
        to_writer_pretty(writer, self).map_err(|err| awsl!(ConfigError, err))?;

        // 覆盖默认配置
        let file = File::create(DEFAULT_CONFIG_PATH)
            .map_err(|err| awsl!(ConfigIOError, err, DEFAULT_CONFIG_PATH))?;
        let writer = BufWriter::new(file);
        to_writer_pretty(writer, self).map_err(|err| awsl!(ConfigError, err))?;

        Ok(())
    }
}

pub fn save_data<P: AsRef<Path>>(data: ArrayView2<f32>, data_path: P) -> TLCResult<()> {
    let mut wtr = WriterBuilder::new()
        .has_headers(false)
        .from_path(data_path.as_ref())
        .map_err(|err| awsl!(DataSaveError, err, data_path.as_ref()))?;

    for row in data.axis_iter(Axis(0)) {
        let v: Vec<_> = row.iter().map(|x| x.to_string()).collect();
        wtr.write_record(&StringRecord::from(v))
            .map_err(|err| awsl!(DataSaveError, err, data_path.as_ref()))?;
    }

    Ok(())
}

pub fn read_data<P: AsRef<Path>>(data_path: P) -> TLCResult<Array2<f32>> {
    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .from_path(data_path.as_ref())
        .map_err(|err| awsl!(DataReadError, err, data_path.as_ref()))?;
    let width = rdr
        .records()
        .next()
        .ok_or(awsl!(DataReadError, "矩阵为空", data_path.as_ref()))?
        .map_err(|err| awsl!(DataReadError, err, data_path.as_ref()))?
        .len();
    let height = rdr.records().count() + 1;

    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .from_path(data_path.as_ref())
        .map_err(|err| awsl!(DataReadError, err, data_path.as_ref()))?;

    let mut data = Array2::zeros((height, width));

    for (csv_row_result, mut nu_row) in rdr.records().zip(data.axis_iter_mut(Axis(0))) {
        let csv_row =
            csv_row_result.map_err(|err| awsl!(DataReadError, err, data_path.as_ref()))?;

        for (csv_val, nu) in csv_row.iter().zip(nu_row.iter_mut()) {
            *nu = csv_val
                .parse::<f32>()
                .map_err(|err| awsl!(DataReadError, err, data_path.as_ref()))?;
        }
    }

    Ok(data)
}
