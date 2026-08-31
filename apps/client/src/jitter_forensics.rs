//! DEV camera-jitter forensics: ring buffer, isolation modes, CSV dump.
//!
//! Diagnosis only. Not simulation, AOI, protocol, or production camera policy.

use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::local_presentation::{FrameLocalPose, offset_length, screen_space_x};

pub const FORENSIC_CAP: usize = 180;

/// DEV isolation modes. Overlay only; default is current product behavior.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CameraJitterMode {
    #[default]
    Normal,
    CameraFrozen,
    PresentationRawPrediction,
    PresentationSmoothed,
    CameraFollowPresented,
    CameraFollowRawPrediction,
}

impl CameraJitterMode {
    pub const ALL: [Self; 6] = [
        Self::Normal,
        Self::CameraFrozen,
        Self::PresentationRawPrediction,
        Self::PresentationSmoothed,
        Self::CameraFollowPresented,
        Self::CameraFollowRawPrediction,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::CameraFrozen => "Camera Frozen",
            Self::PresentationRawPrediction => "Presentation RawPrediction",
            Self::PresentationSmoothed => "Presentation Smoothed",
            Self::CameraFollowPresented => "Camera Follow Presented",
            Self::CameraFollowRawPrediction => "Camera Follow RawPrediction",
        }
    }

    #[must_use]
    pub const fn use_raw_presentation(self) -> bool {
        matches!(self, Self::PresentationRawPrediction)
    }

    #[must_use]
    pub const fn freeze_camera(self) -> bool {
        matches!(self, Self::CameraFrozen)
    }

    #[must_use]
    pub const fn follow_raw_prediction(self) -> bool {
        matches!(self, Self::CameraFollowRawPrediction)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ForensicSample {
    pub frame: u64,
    pub dt: f32,
    pub pred: [f32; 2],
    pub replica: [f32; 2],
    pub offset: [f32; 2],
    pub presented: [f32; 2],
    pub desired: [f32; 2],
    pub camera: [f32; 2],
    pub screen: [f32; 2],
    pub reconciled: bool,
    pub corr_mag: f32,
    pub ticks: u32,
    pub net_frames: u32,
    pub epoch: u32,
    pub replica_seq: u32,
    pub following_x: bool,
    pub fade: &'static str,
    pub tick_alpha: f32,
    pub ndc_x: f32,
    pub pixel_x: f32,
    pub d_pred_x: f32,
    pub d_presented_x: f32,
    pub d_camera_x: f32,
    pub d_screen_x: f32,
    pub d_desired_x: f32,
    pub velocity: [f32; 2],
    pub extra_dx: f32,
    pub interp_alpha: f32,
    pub prev_y: f32,
    pub tick_y: f32,
    pub auth_tick: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct JitterSummary {
    pub pred_x: f32,
    pub replica_x: f32,
    pub presented_x: f32,
    pub cam_x: f32,
    pub desired_x: f32,
    pub screen_x: f32,
    pub corr_mag: f32,
    pub reconciled: bool,
    pub following_x: bool,
    pub offset: [f32; 2],
    pub screen_range: f32,
    pub follow_flips: u32,
    pub max_d_pred_x: f32,
    pub max_d_presented_x: f32,
    pub max_d_camera_x: f32,
    pub max_d_screen_x: f32,
    pub max_d_desired_x: f32,
    pub tick_frames: u32,
    pub idle_frames: u32,
    pub mean_d_presented_on_tick: f32,
    pub mean_d_presented_idle: f32,
    pub extra_dx: f32,
    pub interp_alpha: f32,
    pub prev_y: f32,
    pub tick_y: f32,
    pub presented_y: f32,
    pub velocity: [f32; 2],
    pub auth_tick: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct ForensicPush {
    pub camera: [f32; 2],
    pub desired: [f32; 2],
    pub following_x: bool,
    pub offset: [f32; 2],
    pub dt: f32,
    pub ticks: u32,
    pub net_frames: u32,
    pub epoch: u32,
    pub replica_seq: u32,
    pub fade: &'static str,
    pub tick_alpha: f32,
    pub viewport_width: f32,
    pub window_width: f32,
}

#[derive(Clone, Debug, Default)]
pub struct ForensicTrace {
    samples: VecDeque<ForensicSample>,
    frame_index: u64,
}

impl ForensicTrace {
    pub fn push(&mut self, frame: FrameLocalPose, input: ForensicPush) {
        let pred = frame.predicted.unwrap_or([0.0, 0.0]);
        let replica = frame.replica.unwrap_or([0.0, 0.0]);
        let presented = frame.presented.unwrap_or([0.0, 0.0]);
        let camera = input.camera;
        let screen = [
            screen_space_x(presented[0], camera[0]),
            presented[1] - camera[1],
        ];
        let sx = if input.viewport_width > 1e-6 {
            2.0 / input.viewport_width
        } else {
            0.0
        };
        let ndc_x = screen[0] * sx;
        let pixel_x = (ndc_x * 0.5 + 0.5) * input.window_width;
        let prev = self.samples.back().copied();
        let sample = ForensicSample {
            frame: self.frame_index,
            dt: input.dt,
            pred,
            replica,
            offset: input.offset,
            presented,
            desired: input.desired,
            camera,
            screen,
            reconciled: frame.reconciled,
            corr_mag: offset_length(frame.correction_delta),
            ticks: input.ticks,
            net_frames: input.net_frames,
            epoch: input.epoch,
            replica_seq: input.replica_seq,
            following_x: input.following_x,
            fade: input.fade,
            tick_alpha: input.tick_alpha,
            ndc_x,
            pixel_x,
            d_pred_x: prev.map(|p| pred[0] - p.pred[0]).unwrap_or(0.0),
            d_presented_x: prev.map(|p| presented[0] - p.presented[0]).unwrap_or(0.0),
            d_camera_x: prev.map(|p| camera[0] - p.camera[0]).unwrap_or(0.0),
            d_screen_x: prev.map(|p| screen[0] - p.screen[0]).unwrap_or(0.0),
            d_desired_x: prev.map(|p| input.desired[0] - p.desired[0]).unwrap_or(0.0),
            velocity: frame.velocity,
            extra_dx: frame.extra_dx,
            interp_alpha: frame.interp_alpha,
            prev_y: frame.prev_y,
            tick_y: frame.tick_y,
            auth_tick: frame.auth_tick,
        };
        self.frame_index = self.frame_index.saturating_add(1);
        self.samples.push_back(sample);
        while self.samples.len() > FORENSIC_CAP {
            self.samples.pop_front();
        }
    }

    #[must_use]
    pub fn summary(&self, offset: [f32; 2]) -> JitterSummary {
        let Some(last) = self.samples.back().copied() else {
            return JitterSummary {
                offset,
                ..JitterSummary::default()
            };
        };
        let mut min_s = last.screen[0];
        let mut max_s = last.screen[0];
        let mut flips = 0u32;
        let mut prev_follow = self.samples[0].following_x;
        let mut max_d_pred = 0.0_f32;
        let mut max_d_pres = 0.0_f32;
        let mut max_d_cam = 0.0_f32;
        let mut max_d_scr = 0.0_f32;
        let mut max_d_des = 0.0_f32;
        let mut tick_frames = 0u32;
        let mut idle_frames = 0u32;
        let mut tick_sum = 0.0_f32;
        let mut idle_sum = 0.0_f32;
        for (i, s) in self.samples.iter().enumerate() {
            min_s = min_s.min(s.screen[0]);
            max_s = max_s.max(s.screen[0]);
            max_d_pred = max_d_pred.max(s.d_pred_x.abs());
            max_d_pres = max_d_pres.max(s.d_presented_x.abs());
            max_d_cam = max_d_cam.max(s.d_camera_x.abs());
            max_d_scr = max_d_scr.max(s.d_screen_x.abs());
            max_d_des = max_d_des.max(s.d_desired_x.abs());
            if s.ticks > 0 {
                tick_frames += 1;
                tick_sum += s.d_presented_x.abs();
            } else {
                idle_frames += 1;
                idle_sum += s.d_presented_x.abs();
            }
            if i > 0 && s.following_x != prev_follow {
                flips = flips.saturating_add(1);
            }
            prev_follow = s.following_x;
        }
        JitterSummary {
            pred_x: last.pred[0],
            replica_x: last.replica[0],
            presented_x: last.presented[0],
            cam_x: last.camera[0],
            desired_x: last.desired[0],
            screen_x: last.screen[0],
            corr_mag: last.corr_mag,
            reconciled: last.reconciled,
            following_x: last.following_x,
            offset,
            screen_range: max_s - min_s,
            follow_flips: flips,
            max_d_pred_x: max_d_pred,
            max_d_presented_x: max_d_pres,
            max_d_camera_x: max_d_cam,
            max_d_screen_x: max_d_scr,
            max_d_desired_x: max_d_des,
            tick_frames,
            idle_frames,
            mean_d_presented_on_tick: if tick_frames > 0 {
                tick_sum / tick_frames as f32
            } else {
                0.0
            },
            mean_d_presented_idle: if idle_frames > 0 {
                idle_sum / idle_frames as f32
            } else {
                0.0
            },
            extra_dx: last.extra_dx,
            interp_alpha: last.interp_alpha,
            prev_y: last.prev_y,
            tick_y: last.tick_y,
            presented_y: last.presented[1],
            velocity: last.velocity,
            auth_tick: last.auth_tick,
        }
    }

    pub fn dump_csv(&self) -> Result<PathBuf, String> {
        let dir = Path::new("logs").join("camera_jitter");
        fs::create_dir_all(&dir).map_err(|e| format!("create {dir:?}: {e}"))?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let path = dir.join(format!("jitter-{ts}.csv"));
        let mut file =
            fs::File::create(&path).map_err(|e| format!("create {}: {e}", path.display()))?;
        writeln!(
            file,
            "frame,dt,pred_x,pred_y,replica_x,replica_y,offset_x,offset_y,presented_x,presented_y,desired_x,desired_y,camera_x,camera_y,screen_x,screen_y,reconciled,corr_mag,ticks,net_frames,epoch,replica_seq,following_x,fade,tick_alpha,ndc_x,pixel_x,d_pred_x,d_presented_x,d_camera_x,d_screen_x,d_desired_x,vx,vy,extra_dx,yalpha,prev_y,tick_y,auth_tick"
        )
        .map_err(|e| e.to_string())?;
        for s in &self.samples {
            writeln!(
                file,
                "{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{},{:.6},{},{},{},{},{},{},{:.6},{:.6},{:.4},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{}",
                s.frame,
                s.dt,
                s.pred[0],
                s.pred[1],
                s.replica[0],
                s.replica[1],
                s.offset[0],
                s.offset[1],
                s.presented[0],
                s.presented[1],
                s.desired[0],
                s.desired[1],
                s.camera[0],
                s.camera[1],
                s.screen[0],
                s.screen[1],
                u8::from(s.reconciled),
                s.corr_mag,
                s.ticks,
                s.net_frames,
                s.epoch,
                s.replica_seq,
                u8::from(s.following_x),
                s.fade,
                s.tick_alpha,
                s.ndc_x,
                s.pixel_x,
                s.d_pred_x,
                s.d_presented_x,
                s.d_camera_x,
                s.d_screen_x,
                s.d_desired_x,
                s.velocity[0],
                s.velocity[1],
                s.extra_dx,
                s.interp_alpha,
                s.prev_y,
                s.tick_y,
                s.auth_tick
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera_follow::CameraFollow;
    use crate::local_presentation::FrameLocalPose;
    use crate::renderer::Camera;
    use purgatory_simulation::WorldBounds;

    fn wide() -> WorldBounds {
        WorldBounds {
            min_x: -80.0,
            max_x: 80.0,
            min_y: -20.0,
            max_y: 20.0,
        }
    }

    /// 30 Hz predicted pose + render-rate damped camera → sawtooth screen X.
    #[test]
    fn fixed_tick_pose_plus_damped_camera_makes_screen_sawtooth() {
        let mut cam = Camera {
            position: [2.0, 0.0],
            viewport_width: 16.0,
            viewport_height: 9.0,
        };
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        let mut pred_x = 4.0;
        let vx = 6.0;
        let mut screen = Vec::new();
        let mut presented_idle_max = 0.0_f32;
        let mut presented_tick_max = 0.0_f32;
        let mut prev_p = pred_x;
        let mut trace = ForensicTrace::default();
        for i in 0..120usize {
            let ticked = i.is_multiple_of(2);
            if ticked {
                pred_x += vx * (1.0 / 30.0);
            }
            follow.step(&mut cam, [pred_x, 0.0], wide(), 1.0 / 60.0);
            let dp = (pred_x - prev_p).abs();
            if ticked {
                presented_tick_max = presented_tick_max.max(dp);
            } else {
                presented_idle_max = presented_idle_max.max(dp);
            }
            screen.push(pred_x - cam.position[0]);
            trace.push(
                FrameLocalPose {
                    predicted: Some([pred_x, 0.0]),
                    presented: Some([pred_x, 0.0]),
                    replica: None,
                    velocity: [0.0, 0.0],
                    extra_dx: 0.0,
                    interp_alpha: 0.0,
                    prev_y: 0.0,
                    tick_y: 0.0,
                    auth_tick: 0,
                    replica_seq: 0,
                    correction_delta: [0.0, 0.0],
                    reconciled: false,
                },
                ForensicPush {
                    camera: cam.position,
                    desired: follow.desired,
                    following_x: follow.following_x,
                    offset: [0.0, 0.0],
                    dt: 1.0 / 60.0,
                    ticks: u32::from(ticked),
                    net_frames: 0,
                    epoch: 0,
                    replica_seq: 0,
                    fade: "Idle",
                    tick_alpha: 0.0,
                    viewport_width: 16.0,
                    window_width: 1280.0,
                },
            );
            prev_p = pred_x;
        }
        let max_ds = screen
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0_f32, f32::max);
        assert!(
            presented_idle_max < 1e-5,
            "idle render frames must hold the 30 Hz pose, got {presented_idle_max}"
        );
        assert!(
            presented_tick_max > 0.15,
            "tick frames must step, got {presented_tick_max}"
        );
        assert!(
            max_ds > 0.08,
            "screen X must sawtooth when camera damps between 30 Hz steps, got {max_ds}"
        );
        let summary = trace.summary([0.0, 0.0]);
        assert!(summary.mean_d_presented_idle < summary.mean_d_presented_on_tick * 0.25);
    }
}
