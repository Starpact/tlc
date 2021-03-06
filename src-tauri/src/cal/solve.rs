use std::f32::{consts::PI, NAN};

use libm::erfcf;
use ndarray::prelude::*;
use packed_simd::{f32x8, Simd};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::{error::TLCResult, postprocess, TLCConfig, TLCData};
use crate::awsl;

/// 默认初始对流换热系数
const DEFAULT_H0: f32 = 50.;

/// 默认最大迭代步数
const DEFAULT_MAX_ITER_NUM: usize = 10;

/// 用热电偶温度历史的**前4个**数计算初始温度
const FIRST_FEW_TO_CAL_T0: usize = 4;

/// 迭代方法（初值，最大迭代步数）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum IterationMethod {
    NewtonTangent { h0: f32, max_iter_num: usize },
    NewtonDown { h0: f32, max_iter_num: usize },
}

impl Default for IterationMethod {
    fn default() -> Self {
        Self::NewtonTangent {
            h0: DEFAULT_H0,
            max_iter_num: DEFAULT_MAX_ITER_NUM,
        }
    }
}

/// temporary fake SIMD wrapper of erfcf
fn erfcf_simd(arr: Simd<[f32; 8]>) -> Simd<[f32; 8]> {
    let (x0, x1, x2, x3, x4, x5, x6, x7): (f32, f32, f32, f32, f32, f32, f32, f32) =
        unsafe { std::mem::transmute(arr) };
    f32x8::new(
        erfcf(x0),
        erfcf(x1),
        erfcf(x2),
        erfcf(x3),
        erfcf(x4),
        erfcf(x5),
        erfcf(x6),
        erfcf(x7),
    )
}

/// struct that stores necessary information for solving the equation
struct PointData<'a> {
    peak_frame: usize,
    temps: &'a [f32],
    peak_temp: f32,
    dt: f32,
    solid_thermal_conductivity: f32,
    solid_thermal_diffusivity: f32,
}

impl PointData<'_> {
    /// semi-infinite plate heat transfer equation of each pixel(simd)
    /// ### Return:
    /// equation and its derivative
    fn thermal_equation(&self, h: f32) -> (f32, f32) {
        let (k, a, dt, temps, tw, peak_frame) = (
            self.solid_thermal_conductivity,
            self.solid_thermal_diffusivity,
            self.dt,
            self.temps,
            self.peak_temp,
            self.peak_frame,
        );
        let t0 = temps[..FIRST_FEW_TO_CAL_T0].iter().sum::<f32>() / FIRST_FEW_TO_CAL_T0 as f32;
        let (mut sum, mut diff_sum) = (f32x8::splat(0.), f32x8::splat(0.));

        let mut frame = 1;
        while frame + f32x8::lanes() < peak_frame {
            let delta_temp = unsafe {
                f32x8::from_slice_unaligned_unchecked(&temps[frame..])
                    - f32x8::from_slice_unaligned_unchecked(&temps[frame - 1..])
            };
            let at = a
                * dt
                * f32x8::new(
                    (peak_frame - frame) as f32,
                    (peak_frame - frame - 1) as f32,
                    (peak_frame - frame - 2) as f32,
                    (peak_frame - frame - 3) as f32,
                    (peak_frame - frame - 4) as f32,
                    (peak_frame - frame - 5) as f32,
                    (peak_frame - frame - 6) as f32,
                    (peak_frame - frame - 7) as f32,
                );
            let exp_erfc = (h.powf(2.) / k.powf(2.) * at).exp() * erfcf_simd(h / k * at.sqrt());
            let step = (1. - exp_erfc) * delta_temp;
            let d_step = -delta_temp
                * (2. * at.sqrt() / k / PI.sqrt() - (2. * at * h * exp_erfc) / k.powf(2.));

            frame += f32x8::lanes();
            sum += step;
            diff_sum += d_step;
        }

        let (mut sum, mut diff_sum) = (sum.sum(), diff_sum.sum());

        while frame < peak_frame {
            let delta_temp = unsafe { temps.get_unchecked(frame) - temps.get_unchecked(frame - 1) };
            let at = a * dt * (peak_frame - frame) as f32;
            let exp_erfc = (h.powf(2.) / k.powf(2.) * at).exp() * erfcf(h / k * at.sqrt());
            let step = (1. - exp_erfc) * delta_temp;
            let d_step = -delta_temp
                * (2. * at.sqrt() / k / PI.sqrt() - (2. * at * h * exp_erfc) / k.powf(2.));

            frame += 1;
            sum += step;
            diff_sum += d_step;
        }

        (tw - t0 - sum, diff_sum)
    }
}

fn newton_tangent(h0: f32, max_iter_num: usize) -> impl Fn(PointData) -> f32 {
    move |point_data| {
        let mut h = h0;
        for _ in 0..max_iter_num {
            let (f, df) = point_data.thermal_equation(h);
            let next_h = h - f / df;
            if next_h.abs() > 10000. {
                return NAN;
            }
            if (next_h - h).abs() < 1e-3 {
                return next_h;
            }
            h = next_h;
        }

        h
    }
}

fn newton_down(h0: f32, max_iter_num: usize) -> impl Fn(PointData) -> f32 {
    move |point_data| {
        let mut h = h0;
        let (mut f, mut df) = point_data.thermal_equation(h);
        for _ in 0..max_iter_num {
            let mut lambda = 1.;
            loop {
                let next_h = h - lambda * f / df;
                if (next_h - h).abs() < 1e-3 {
                    return next_h;
                }
                let (next_f, next_df) = point_data.thermal_equation(next_h);
                if next_f.abs() < f.abs() {
                    h = next_h;
                    f = next_f;
                    df = next_df;
                    break;
                }
                lambda /= 2.;
                if lambda < 1e-3 {
                    return NAN;
                }
            }
            if h.abs() > 10000. {
                return NAN;
            }
        }

        h
    }
}

impl TLCData {
    pub fn solve(&mut self) -> TLCResult<&mut Self> {
        if self.peak_frames.is_none() {
            self.detect_peak()?;
        }
        if self.interp.is_none() {
            self.interp()?;
        }

        use IterationMethod::*;
        match self.config.iteration_method {
            NewtonTangent { h0, max_iter_num } => self.solve_core(newton_tangent(h0, max_iter_num)),
            NewtonDown { h0, max_iter_num } => self.solve_core(newton_down(h0, max_iter_num)),
        }
    }

    fn solve_core<F>(&mut self, f: F) -> TLCResult<&mut Self>
    where
        F: Fn(PointData) -> f32 + Send + Sync,
    {
        let peak_frames = self.get_peak_frames()?;
        let interp = self.get_interp()?;

        let TLCConfig {
            region_shape,
            frame_rate,
            peak_temp,
            solid_thermal_conductivity,
            solid_thermal_diffusivity,
            characteristic_length,
            air_thermal_conductivity,
            ..
        } = self.config;
        let dt = 1. / frame_rate as f32;

        let mut nus = vec![0.; peak_frames.len()];

        peak_frames
            .into_par_iter()
            .enumerate()
            .zip(nus.par_iter_mut())
            .try_for_each(|((pos, &peak_frame), nu)| -> Option<()> {
                *nu = if peak_frame > FIRST_FEW_TO_CAL_T0 {
                    let temps = interp.interp_single_point(pos, region_shape);
                    let temps = temps.as_slice_memory_order()?;
                    let point_data = PointData {
                        peak_frame,
                        temps,
                        peak_temp,
                        dt,
                        solid_thermal_conductivity,
                        solid_thermal_diffusivity,
                    };
                    f(point_data) * characteristic_length / air_thermal_conductivity
                } else {
                    NAN
                };

                Some(())
            })
            .ok_or(awsl!())?;

        let mut nu2d = Array1::from(nus)
            .into_shape(region_shape)
            .map_err(|err| awsl!(err))?;
        nu2d.invert_axis(Axis(0));
        self.nu_nan_mean
            .insert(postprocess::cal_nan_mean(nu2d.view()));
        self.nu2d.insert(nu2d);

        Ok(self)
    }
}
