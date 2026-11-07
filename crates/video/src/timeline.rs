use crate::VideoError;

const MAX_KEYFRAMES: usize = 64;
const MAX_ELEVATION: f32 = 89.9;

/// Interpolation applied to every keyframe segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    Smooth,
}

impl Easing {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "linear" => Some(Self::Linear),
            "smooth" => Some(Self::Smooth),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Smooth => "smooth",
        }
    }

    fn apply(self, t: f64) -> f64 {
        match self {
            Self::Linear => t,
            Self::Smooth => t * t * (3.0 - 2.0 * t),
        }
    }
}

/// One camera sample. Yaw is intentionally not wrapped: `350 -> 370` is a
/// short forward turn while `350 -> 10` rotates backward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    pub time: f64,
    pub yaw: f32,
    pub elevation: f32,
}

impl Keyframe {
    pub const fn new(time: f64, yaw: f32, elevation: f32) -> Self {
        Self {
            time,
            yaw,
            elevation,
        }
    }
}

/// Validated keyframes plus the sampling policy.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    keyframes: Vec<Keyframe>,
    duration: f64,
    fps: u32,
    looped: bool,
    easing: Easing,
}

impl Animation {
    pub fn new(
        keyframes: Vec<Keyframe>,
        duration: Option<f64>,
        fps: u32,
        looped: bool,
        easing: Easing,
    ) -> Result<Self, VideoError> {
        if keyframes.is_empty() {
            return Err(VideoError::animation("at least one keyframe is required"));
        }
        if keyframes.len() > MAX_KEYFRAMES {
            return Err(VideoError::animation(format!(
                "at most {MAX_KEYFRAMES} keyframes are supported"
            )));
        }
        if !(1..=60).contains(&fps) {
            return Err(VideoError::animation("fps must be between 1 and 60"));
        }

        let mut previous_time = None;
        for keyframe in &keyframes {
            if !keyframe.time.is_finite() || keyframe.time < 0.0 {
                return Err(VideoError::animation(
                    "keyframe time must be finite and non-negative",
                ));
            }
            if previous_time.is_some_and(|previous| keyframe.time <= previous) {
                return Err(VideoError::animation(
                    "keyframe times must be strictly increasing",
                ));
            }
            previous_time = Some(keyframe.time);
            if !keyframe.yaw.is_finite() {
                return Err(VideoError::animation("keyframe yaw must be finite"));
            }
            if !keyframe.elevation.is_finite() || keyframe.elevation.abs() > MAX_ELEVATION {
                return Err(VideoError::animation(format!(
                    "keyframe elevation must be within -{MAX_ELEVATION}..={MAX_ELEVATION}"
                )));
            }
        }

        let last_time = keyframes.last().expect("keyframes are non-empty").time;
        let duration = duration.unwrap_or(last_time);
        if !duration.is_finite() || duration <= 0.0 {
            return Err(VideoError::animation(
                "duration must be finite and positive",
            ));
        }
        if duration < last_time {
            return Err(VideoError::animation(
                "duration must cover the last keyframe",
            ));
        }

        let sampled = duration * f64::from(fps);
        if !sampled.is_finite() || sampled > f64::from(u32::MAX) {
            return Err(VideoError::animation(
                "duration and fps overflow the frame plan",
            ));
        }
        let mut frame_count = sampled.floor() as u32;
        if !looped {
            frame_count = frame_count.saturating_add(1);
        }
        if looped && frame_count < 2 {
            return Err(VideoError::animation(
                "a looping animation needs at least two frames",
            ));
        }
        if !looped && frame_count == 0 {
            return Err(VideoError::animation(
                "a non-looping animation needs at least one frame",
            ));
        }

        Ok(Self {
            keyframes,
            duration,
            fps,
            looped,
            easing,
        })
    }

    pub fn frame_plan(&self) -> FramePlan {
        let sampled = self.duration * f64::from(self.fps);
        let mut frame_count = sampled.floor() as u32;
        if !self.looped {
            frame_count = frame_count.saturating_add(1);
        }
        FramePlan {
            animation: self.clone(),
            frame_count,
        }
    }

    pub fn duration(&self) -> f64 {
        self.duration
    }

    pub fn fps(&self) -> u32 {
        self.fps
    }

    pub const fn looped(&self) -> bool {
        self.looped
    }

    pub const fn easing(&self) -> Easing {
        self.easing
    }

    pub fn keyframes(&self) -> &[Keyframe] {
        &self.keyframes
    }

    /// Camera at `time`, clamped to the first/last keyframe outside the timeline.
    pub fn camera_at(&self, time: f64) -> (f32, f32) {
        let first = self.keyframes[0];
        let last = self.keyframes[self.keyframes.len() - 1];
        if time <= first.time {
            return (first.yaw, first.elevation);
        }
        if time >= last.time {
            return (last.yaw, last.elevation);
        }

        let next = self
            .keyframes
            .partition_point(|keyframe| keyframe.time <= time);
        let from = self.keyframes[next - 1];
        let to = self.keyframes[next];
        let span = to.time - from.time;
        let t = if span <= f64::EPSILON {
            1.0
        } else {
            (time - from.time) / span
        };
        let t = self.easing.apply(t.clamp(0.0, 1.0));
        let yaw = f64::from(from.yaw) + (f64::from(to.yaw) - f64::from(from.yaw)) * t;
        let elevation =
            f64::from(from.elevation) + (f64::from(to.elevation) - f64::from(from.elevation)) * t;
        (yaw as f32, elevation as f32)
    }
}

/// Deterministic frame count and sample times.
#[derive(Debug, Clone, PartialEq)]
pub struct FramePlan {
    animation: Animation,
    frame_count: u32,
}

impl FramePlan {
    pub const fn len(&self) -> u32 {
        self.frame_count
    }

    pub const fn is_empty(&self) -> bool {
        self.frame_count == 0
    }

    pub const fn fps(&self) -> u32 {
        self.animation.fps
    }

    pub fn duration(&self) -> f64 {
        self.animation.duration
    }

    pub const fn looped(&self) -> bool {
        self.animation.looped
    }

    pub fn sample_time(&self, index: u32) -> Option<f64> {
        (index < self.frame_count).then(|| f64::from(index) / f64::from(self.animation.fps))
    }

    pub fn camera_at(&self, index: u32) -> Option<(f32, f32)> {
        self.sample_time(index)
            .map(|time| self.animation.camera_at(time))
    }

    pub fn sample_times(&self) -> impl Iterator<Item = f64> + '_ {
        (0..self.frame_count).map(|index| f64::from(index) / f64::from(self.animation.fps))
    }
}

#[cfg(test)]
mod tests {
    use super::{Animation, Easing, Keyframe};

    fn animation(
        keyframes: Vec<Keyframe>,
        duration: Option<f64>,
        fps: u32,
        looped: bool,
    ) -> Animation {
        Animation::new(keyframes, duration, fps, looped, Easing::Linear).unwrap()
    }

    #[test]
    fn loop_plan_excludes_the_duplicate_endpoint() {
        let animation = animation(
            vec![
                Keyframe::new(0.0, 45.0, 30.0),
                Keyframe::new(2.0, 405.0, 30.0),
            ],
            None,
            10,
            true,
        );
        let plan = animation.frame_plan();
        assert_eq!(plan.len(), 20);
        assert_eq!(plan.sample_time(0), Some(0.0));
        assert_eq!(plan.sample_time(19), Some(1.9));
        assert_eq!(plan.sample_time(20), None);
    }

    #[test]
    fn non_loop_plan_includes_the_end_sample() {
        let animation = animation(
            vec![
                Keyframe::new(0.0, 45.0, 30.0),
                Keyframe::new(2.0, 405.0, 30.0),
            ],
            None,
            10,
            false,
        );
        let plan = animation.frame_plan();
        assert_eq!(plan.len(), 21);
        assert_eq!(plan.sample_time(20), Some(2.0));
    }

    #[test]
    fn fractional_duration_stops_at_the_last_whole_frame() {
        let animation = animation(vec![Keyframe::new(0.0, 45.0, 30.0)], Some(1.05), 10, false);
        let plan = animation.frame_plan();
        assert_eq!(plan.len(), 11);
        assert_eq!(plan.sample_time(10), Some(1.0));
    }

    #[test]
    fn yaw_interpolates_without_wrapping() {
        let animation = animation(
            vec![
                Keyframe::new(0.0, 350.0, 0.0),
                Keyframe::new(1.0, 10.0, 0.0),
            ],
            None,
            2,
            true,
        );
        let (yaw, _) = animation.camera_at(0.5);
        assert!((yaw - 180.0).abs() < 1e-5, "yaw was {yaw}");
        let (yaw, _) = animation.camera_at(10.0);
        assert!((yaw - 10.0).abs() < 1e-5);
    }

    #[test]
    fn smooth_easing_is_symmetric_and_clamped() {
        let animation = Animation::new(
            vec![Keyframe::new(0.0, 0.0, 0.0), Keyframe::new(1.0, 100.0, 0.0)],
            None,
            10,
            true,
            Easing::Smooth,
        )
        .unwrap();
        let (quarter, _) = animation.camera_at(0.25);
        let (three_quarter, _) = animation.camera_at(0.75);
        assert!((quarter - 15.625).abs() < 1e-5);
        assert!((three_quarter - 84.375).abs() < 1e-5);
        assert_eq!(animation.camera_at(-1.0), (0.0, 0.0));
        assert_eq!(animation.camera_at(2.0), (100.0, 0.0));
    }

    #[test]
    fn invalid_keyframes_are_rejected() {
        assert!(Animation::new(vec![], None, 10, true, Easing::Linear).is_err());
        assert!(
            Animation::new(
                vec![Keyframe::new(1.0, 0.0, 0.0), Keyframe::new(1.0, 0.0, 0.0)],
                None,
                10,
                true,
                Easing::Linear
            )
            .is_err()
        );
        assert!(
            Animation::new(
                vec![Keyframe::new(0.0, 0.0, 90.0)],
                None,
                10,
                false,
                Easing::Linear
            )
            .is_err()
        );
        assert!(
            Animation::new(
                vec![Keyframe::new(2.0, 0.0, 0.0)],
                Some(1.0),
                10,
                true,
                Easing::Linear
            )
            .is_err()
        );
        assert!(
            Animation::new(
                vec![Keyframe::new(0.0, 0.0, 0.0)],
                None,
                10,
                true,
                Easing::Linear
            )
            .is_err()
        );
    }
}
