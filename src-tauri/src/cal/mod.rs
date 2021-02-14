mod colormap;
pub mod error;
pub mod io;
pub mod postprocess;
pub mod preprocess;
pub mod solve;

use std::path::Path;

use error::TLCResult;

use serde::{Deserialize, Serialize};

use preprocess::{FilterMethod, Interp, InterpMethod};

use ndarray::prelude::*;
use solve::IterationMethod;

use crate::err;

/// 默认配置文件路径
const DEFAULT_CONFIG_PATH: &'static str = "./cache/default_config.json";

/// 所有配置信息，与case一一对应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TLCConfig {
    #[serde(default)]
    /// 实验组名称（与视频文件名一致）
    case_name: String,

    /// 保存配置信息和所有结果的根目录
    save_dir: String,
    /// 视频文件路径
    video_path: String,
    /// 数采文件路径
    daq_path: String,
    /// 配置文件保存路径（仅运行时使用）
    #[serde(default)]
    config_path: String,
    /// 图片保存路径
    #[serde(default)]
    plots_path: String,
    /// 数据保存路径
    #[serde(default)]
    data_path: String,

    /// 视频起始帧数
    start_frame: usize,
    /// 视频总帧数
    #[serde(default)]
    total_frames: usize,
    /// 视频帧率
    #[serde(default)]
    frame_rate: usize,
    /// 数采文件起始行数
    start_row: usize,
    /// 数采文件总行数
    #[serde(default)]
    total_rows: usize,
    /// 实际处理总帧数
    #[serde(default)]
    frame_num: usize,

    /// 计算区域左上角坐标(y, x)
    top_left_pos: (usize, usize),
    /// 计算区域尺寸（高，宽）
    region_shape: (usize, usize),
    /// 各热电偶对应的数采文件中的列数
    temp_column_num: Vec<usize>,
    /// 各热电偶位置
    thermocouple_pos: Vec<(i32, i32)>,
    /// 插值方法
    interp_method: InterpMethod,
    /// 滤波方法
    filter_method: FilterMethod,
    /// 导热方程迭代求解方法（初值，最大迭代步数）
    #[serde(default)]
    iteration_method: IterationMethod,

    /// 峰值温度
    peak_temp: f32,
    /// 固体导热系数
    solid_thermal_conductivity: f32,
    /// 固体热扩散系数
    solid_thermal_diffusivity: f32,
    /// 特征长度
    characteristic_length: f32,
    /// 空气导热系数
    air_thermal_conductivity: f32,

    /// 参考温度调节系数
    #[serde(default)]
    regulator: Vec<f32>,
}

/// 配置信息 + 运行时数据
///
/// 运行时产生的数据会在内存可能不足时或所依赖配置变化时析构
#[derive(Debug)]
pub struct TLCData {
    /// 配置信息
    config: TLCConfig,
    /// 未滤波的Green值二维矩阵，排列方式如下：
    ///
    /// 第一帧: | X1Y1 X2Y1 ... XnY1 X1Y2 X2Y2 ... XnY2 ... |
    ///
    /// 第二帧: | X1Y1 X2Y1 ... XnY1 X1Y2 X2Y2 ... XnY2 ... |
    ///
    /// ......
    raw_g2d: Option<Array2<u8>>,
    /// 滤波后的Green值二维矩阵
    filtered_g2d: Option<Array2<u8>>,
    /// 所有点峰值对应帧数
    peak_frames: Option<Vec<usize>>,
    /// 热电偶温度二维矩阵，排列方式如下：
    ///
    /// 1号热电偶：| 第一帧 第二帧 ... |
    ///
    /// 2号热电偶：| 第一帧 第二帧 ... |
    ///
    /// ......
    t2d: Option<Array2<f32>>,
    /// 插值所得温度场
    interp: Option<Interp>,
    /// 努塞尔数二维矩阵
    nu2d: Option<Array2<f32>>,
    /// 努赛尔数平均值
    nu_ave: Option<f32>,
}

/// 当某项数据所依赖的配置信息发生变化时，清空数据
macro_rules! delete {
    ($v:ident @ all) => {
        delete!($v @ raw_g2d, filtered_g2d, t2d, interp, nu2d, nu_ave);
    };

    ($v:ident @ $($member:tt),* $(,)*) => {
        $($v.$member = None;)*
    };
}

impl TLCData {
    pub fn new() -> TLCResult<Self> {
        Self::from_path(DEFAULT_CONFIG_PATH)
    }

    pub fn from_path<P: AsRef<Path>>(config_path: P) -> TLCResult<Self> {
        Ok(Self {
            config: TLCConfig::from_path(config_path)?,
            raw_g2d: None,
            filtered_g2d: None,
            peak_frames: None,
            t2d: None,
            interp: None,
            nu2d: None,
            nu_ave: None,
        })
    }

    pub fn get_config(&self) -> &'_ TLCConfig {
        &self.config
    }

    pub fn get_raw_g2d(&self) -> Option<ArrayView2<'_, u8>> {
        Some(self.raw_g2d.as_ref()?.view())
    }

    pub fn get_filtered_g2d(&self) -> Option<ArrayView2<'_, u8>> {
        Some(self.filtered_g2d.as_ref()?.view())
    }

    pub fn get_peak_frames(&self) -> Option<&'_ Vec<usize>> {
        self.peak_frames.as_ref()
    }

    pub fn get_t2d(&self) -> Option<ArrayView2<'_, f32>> {
        Some(self.t2d.as_ref()?.view())
    }

    pub fn get_nu2d(&self) -> Option<ArrayView2<'_, f32>> {
        Some(self.nu2d.as_ref()?.view())
    }

    pub fn get_nu_ave(&self) -> Option<f32> {
        self.nu_ave
    }

    pub fn set_save_dir(&mut self, save_dir: String) -> TLCResult<&mut Self> {
        self.config.set_save_dir(save_dir)?;

        Ok(self)
    }

    pub fn set_video_path(&mut self, video_path: String) -> TLCResult<&mut Self> {
        self.config.set_video_path(video_path)?;
        delete!(self @ raw_g2d, filtered_g2d, peak_frames, nu2d, nu_ave);

        Ok(self)
    }

    pub fn set_daq_path(&mut self, daq_path: String) -> TLCResult<&mut Self> {
        self.config.set_daq_path(daq_path)?;
        delete!(self @ t2d, interp, nu2d, nu_ave);

        Ok(self)
    }

    pub fn set_filter_method(&mut self, filter_method: FilterMethod) -> &mut Self {
        self.config.filter_method = filter_method;
        delete!(self @ filtered_g2d, peak_frames, nu2d, nu_ave);

        self
    }

    pub fn set_interp_method(&mut self, interp_method: InterpMethod) -> &mut Self {
        self.config.interp_method = interp_method;
        delete!(self @ interp, nu2d, nu_ave);

        self
    }

    pub fn set_iteration_method(&mut self, iteration_method: IterationMethod) -> &mut Self {
        self.config.iteration_method = iteration_method;
        delete!(self @ nu2d, nu_ave);

        self
    }

    pub fn set_region(
        &mut self,
        top_left_pos: (usize, usize),
        region_shape: (usize, usize),
    ) -> &mut Self {
        self.config.top_left_pos = top_left_pos;
        self.config.region_shape = region_shape;
        delete!(self @ all);

        self
    }

    pub fn set_regulator(&mut self, regulator: Vec<f32>) -> &mut Self {
        self.config.regulator = regulator;
        delete!(self @ t2d, interp, nu2d, nu_ave);

        self
    }

    pub fn set_peak_temp(&mut self, peak_temp: f32) -> &mut Self {
        self.config.peak_temp = peak_temp;
        delete!(self @ nu2d, nu_ave);

        self
    }

    pub fn set_solid_thermal_conductivity(&mut self, solid_thermal_conductivity: f32) -> &mut Self {
        self.config.solid_thermal_conductivity = solid_thermal_conductivity;
        delete!(self @ nu2d, nu_ave);

        self
    }

    pub fn set_solid_thermal_diffusivity(&mut self, solid_thermal_diffusivity: f32) -> &mut Self {
        self.config.solid_thermal_diffusivity = solid_thermal_diffusivity;
        delete!(self @ nu2d, nu_ave);

        self
    }

    pub fn set_air_thermal_conductivity(&mut self, air_thermal_conductivity: f32) -> &mut Self {
        self.config.air_thermal_conductivity = air_thermal_conductivity;
        delete!(self @ nu2d, nu_ave);

        self
    }

    pub fn set_characteristic_length(&mut self, characteristic_length: f32) -> &mut Self {
        self.config.characteristic_length = characteristic_length;
        delete!(self @ nu2d, nu_ave);

        self
    }

    pub fn set_start_frame(&mut self, start_frame: usize) -> &mut Self {
        self.config.start_frame = start_frame;
        delete!(self @ all);

        self
    }

    pub fn set_start_row(&mut self, start_row: usize) -> &mut Self {
        self.config.start_row = start_row;
        delete!(self @ all);

        self
    }

    pub fn set_temp_column_num(&mut self, temp_column_num: Vec<usize>) -> &mut Self {
        self.config.temp_column_num = temp_column_num;
        delete!(self @ t2d, interp, nu2d, nu_ave);

        self
    }

    pub fn set_thermocouple_pos(&mut self, thermocouple_pos: Vec<(i32, i32)>) -> &mut Self {
        self.config.thermocouple_pos = thermocouple_pos;
        delete!(self @ interp, nu2d, nu_ave);

        self
    }

    pub fn read_video(&mut self) -> TLCResult<&mut Self> {
        self.raw_g2d.get_or_insert(self.config.read_video()?);

        Ok(self)
    }

    pub fn read_daq(&mut self) -> TLCResult<&mut Self> {
        self.t2d.get_or_insert(self.config.read_daq()?);

        Ok(self)
    }

    pub fn save_config(&self) -> TLCResult<&Self> {
        self.config.save()?;

        Ok(self)
    }

    pub fn save_nu(&self) -> TLCResult<&Self> {
        let nu2d = self.nu2d.as_ref().ok_or(err!())?.view();
        io::save_data(nu2d, &self.config.data_path)?;

        Ok(self)
    }

    pub fn plot_nu(&self) -> TLCResult<&Self> {
        let nu_nan_mean = self.nu_ave.ok_or(err!())?;
        postprocess::plot_area(
            self.nu2d.as_ref().ok_or(err!())?.view(),
            nu_nan_mean * 0.6,
            nu_nan_mean * 2.,
            &self.config.plots_path,
        )?;

        Ok(self)
    }

    pub fn plot_temps_single_frame(&self, frame: usize) -> TLCResult<()> {
        let temps_single_frame = self.interp_single_frame(frame)?;
        let mean = postprocess::cal_average(temps_single_frame.view());
        postprocess::plot_area(
            temps_single_frame.view(),
            mean * 0.5,
            mean * 1.2,
            "./tmp/plots/temps.png",
        )?;

        Ok(())
    }
}