//! Fixed logical frontend framing and deterministic presentation camera.

pub(crate) const SCENE_DIMENSIONS: [u32; 2] = [2880, 5120];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrontendSceneStop {
    Intro,
    Login,
    Channel,
    Character,
}

impl FrontendSceneStop {
    fn y(self) -> f32 {
        match self {
            Self::Intro => 0.0,
            Self::Login => 1227.0,
            Self::Channel => 2453.0,
            Self::Character => 3680.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Transition {
    source_y: f32,
    destination_y: f32,
    elapsed: f32,
    duration: f32,
}

pub(crate) struct FrontendScene {
    camera_y: f32,
    target: FrontendSceneStop,
    transition: Option<Transition>,
}

impl FrontendScene {
    pub(crate) fn new() -> Self {
        Self {
            camera_y: 0.0,
            target: FrontendSceneStop::Intro,
            transition: None,
        }
    }

    pub(crate) fn request(&mut self, stop: FrontendSceneStop) {
        if stop == self.target {
            return;
        }
        self.target = stop;
        self.transition = Some(Transition {
            source_y: self.camera_y,
            destination_y: stop.y(),
            elapsed: 0.0,
            duration: 1.0,
        });
    }

    pub(crate) fn advance(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let Some(transition) = self.transition.as_mut() else {
            return;
        };
        transition.elapsed = (transition.elapsed + dt).min(transition.duration);
        let t = transition.elapsed / transition.duration;
        let eased = t * t * (3.0 - 2.0 * t);
        self.camera_y =
            transition.source_y + (transition.destination_y - transition.source_y) * eased;
        if transition.elapsed == transition.duration {
            self.camera_y = transition.destination_y;
            self.transition = None;
        }
    }

    pub(crate) fn source_rect(&self) -> [[f32; 2]; 2] {
        [[160.0, self.camera_y], [2720.0, self.camera_y + 1440.0]]
    }

    pub(crate) fn uv_rect(&self) -> [[f32; 2]; 2] {
        self.source_rect().map(|[x, y]| {
            [
                x / SCENE_DIMENSIONS[0] as f32,
                y / SCENE_DIMENSIONS[1] as f32,
            ]
        })
    }
}

pub(crate) fn validate_dimensions(dimensions: [u32; 2]) -> Result<(), String> {
    if dimensions == SCENE_DIMENSIONS {
        Ok(())
    } else {
        Err(format!(
            "frontend.scene.guide: expected 2880x5120 PNG, got {}x{}",
            dimensions[0], dimensions[1]
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const STOPS: [FrontendSceneStop; 4] = [
        FrontendSceneStop::Intro,
        FrontendSceneStop::Login,
        FrontendSceneStop::Channel,
        FrontendSceneStop::Character,
    ];

    #[test]
    fn stops_and_uvs_fit_locked_scene() {
        let mut scene = FrontendScene::new();
        assert_eq!(scene.uv_rect()[0][1], 0.0);
        for stop in STOPS {
            scene.request(stop);
            scene.advance(1.0);
            let [min, max] = scene.source_rect();
            assert_eq!([min[0], max[0]], [160.0, 2720.0]);
            assert_eq!([min[1], max[1]], [stop.y(), stop.y() + 1440.0]);
            assert!(min[1] >= 0.0 && max[1] <= 5120.0);
            assert!(
                scene
                    .uv_rect()
                    .into_iter()
                    .flatten()
                    .all(|v| (0.0..=1.0).contains(&v))
            );
        }
        assert_eq!(scene.uv_rect()[1][1], 1.0);
    }

    #[test]
    fn transitions_are_exact_bounded_and_monotonic_in_both_directions() {
        for from in STOPS {
            for to in STOPS {
                let mut scene = FrontendScene::new();
                scene.request(from);
                scene.advance(1.0);
                scene.request(to);
                assert_eq!(scene.camera_y, from.y());
                let mut previous = scene.camera_y;
                for _ in 0..8 {
                    scene.advance(0.125);
                    assert!(
                        (from.y().min(to.y())..=from.y().max(to.y())).contains(&scene.camera_y)
                    );
                    assert!((scene.camera_y - previous) * (to.y() - from.y()) >= 0.0);
                    previous = scene.camera_y;
                }
                assert_eq!(scene.camera_y, to.y());
                assert!(scene.transition.is_none());
            }
        }
    }

    #[test]
    fn repeated_requests_preserve_progress_and_current_stop_is_noop() {
        let mut scene = FrontendScene::new();
        scene.request(FrontendSceneStop::Intro);
        assert!(scene.transition.is_none());
        scene.request(FrontendSceneStop::Character);
        scene.advance(0.4);
        let transition = scene.transition;
        scene.request(FrontendSceneStop::Character);
        assert_eq!(scene.transition, transition);
        scene.advance(0.6);
        assert_eq!(scene.camera_y, 3680.0);
        scene.request(FrontendSceneStop::Character);
        assert!(scene.transition.is_none());
    }

    #[test]
    fn retarget_starts_at_current_position_and_invalid_dt_is_ignored() {
        let mut scene = FrontendScene::new();
        scene.request(FrontendSceneStop::Character);
        scene.advance(0.5);
        let y = scene.camera_y;
        scene.request(FrontendSceneStop::Intro);
        assert_eq!(scene.camera_y, y);
        for dt in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            scene.advance(dt);
        }
        assert_eq!(scene.camera_y, y);
        scene.advance(5.0);
        assert_eq!(scene.camera_y, 0.0);
    }

    #[test]
    fn dimension_validation() {
        assert!(validate_dimensions([2880, 5120]).is_ok());
        for dimensions in [[5120, 2880], [2880, 5119], [0, 0]] {
            assert!(
                validate_dimensions(dimensions)
                    .unwrap_err()
                    .contains("expected 2880x5120")
            );
        }
    }
}
