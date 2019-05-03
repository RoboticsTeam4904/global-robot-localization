use crate::robot::map::Map2D;
use crate::robot::Sensor;
use crate::utility::{Pose, Point};
use super::DistanceSensor;
use rand::distributions::{Distribution, Normal};
use rand::thread_rng;

pub struct DummyDistanceSensor {
    noise_distr: Normal,
    relative_pose: Pose,
    pub map: Map2D,
    pub robot_pose: Pose,
    pub max_dist: Option<f64>,
}

impl DummyDistanceSensor {
    /// Noise is Guassian and `noise_margin` is each equal to three standard deviations of the noise distribution.
    /// `relative_pose` is the pose of the sensor relative to the robot
    pub fn new(noise_margin: f64, relative_pose: Pose, map: Map2D, robot_pose: Pose, max_dist: Option<f64>) -> Self {
        Self {
            noise_distr: Normal::new(0., noise_margin / 3.),
            relative_pose,
            map,
            robot_pose,
            max_dist
        }
    }

    pub fn update_pose(&mut self, new_pose: Pose) {
        self.robot_pose = new_pose
    }
}

impl Sensor<Option<f64>> for DummyDistanceSensor {
    fn sense(&self) -> Option<f64> {
        let sensor_pose = self.relative_pose + self.robot_pose;
        let dist = self
            .map
            .raycast(sensor_pose)?
            .dist(sensor_pose.position);
        if let Some(max_dist) = self.max_dist {
            if dist > max_dist {
                None
            } else {
                Some(dist + self.noise_distr.sample(&mut thread_rng()))
            }
        } else {
            Some(dist + self.noise_distr.sample(&mut thread_rng()))
        }
    }
}

impl DistanceSensor<Option<f64>> for DummyDistanceSensor {
    fn get_relative_pose(&self) -> Pose  {
        self.relative_pose
    }
}

pub struct DummyMotionSensor {
    angle_noise_distr: Normal,
    x_noise_distr: Normal,
    y_noise_distr: Normal,
    prev_robot_pose: Pose,
    pub robot_pose: Pose,
}

impl DummyMotionSensor {
    /// Noise is Guassian and `noise_margins` are each equal to three standard deviations of the noise distributions
    pub fn new(robot_pose: Pose, noise_margins: Pose) -> Self {
        Self {
            angle_noise_distr: Normal::new(0., noise_margins.angle / 3.),
            x_noise_distr: Normal::new(0., noise_margins.position.x / 3.),
            y_noise_distr: Normal::new(0., noise_margins.position.y / 3.),
            prev_robot_pose: robot_pose,
            robot_pose,
        }
    }

    pub fn update_pose(&mut self, new_pose: Pose) {
        self.prev_robot_pose = self.robot_pose;
        self.robot_pose = new_pose
    }
}

impl Sensor<Pose> for DummyMotionSensor {
    fn sense(&self) -> Pose {
        let mut rng = thread_rng();
        self.robot_pose - self.prev_robot_pose
            + Pose {
                angle: self.angle_noise_distr.sample(&mut rng),
                position: Point {
                    x: self.x_noise_distr.sample(&mut rng),
                    y: self.y_noise_distr.sample(&mut rng),
                },
            }
    }
}
