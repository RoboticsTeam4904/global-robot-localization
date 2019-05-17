use crate::robot::map::Map2D;
use crate::robot::sensors::LimitedSensor;
use crate::utility::{Point, Pose};
use crate::robot::sensors::Sensor;
use rand::distributions::WeightedIndex;
use rand::prelude::*;
use std::f64::consts::PI;
use std::sync::Arc;

/// A pose localizer that uses likelyhood-based Monte Carlo Localization
/// and takes in motion and range finder sensor data
pub struct DistanceFinderMCL {
    pub map: Arc<Map2D>,
    pub belief: Vec<Pose>,
    max_particle_count: usize,
    weight_sum_threshold: f64,
    sensor_poses: Vec<Pose>,
    weight_from_error: Box<dyn FnMut(&f64) -> f64 + Send + Sync>,
    resampling_noise: Pose,
}

impl DistanceFinderMCL {
    /// Generates a new localizer with the given parameters.
    /// Every step, the localizer should recieve a control and observation update
    pub fn new(
        max_particle_count: usize,
        map: Arc<Map2D>,
        sensor_poses: Vec<Pose>,
        weight_from_error: Box<dyn FnMut(&f64) -> f64 + Send + Sync>,
        resampling_noise: Pose,
    ) -> Self {
        let mut belief = Vec::with_capacity(max_particle_count);
        for _ in 0..max_particle_count {
            belief.push(Pose::random(0.0..2. * PI, 0.0..map.width, 0.0..map.height));
        }
        Self {
            max_particle_count,
            weight_sum_threshold: max_particle_count as f64 / 50., // TODO: fixed parameter
            map,
            sensor_poses,
            weight_from_error,
            belief,
            resampling_noise,
        }
    }

    /// Similar to new, but instead of generating `belief` based on a uniform distribution,
    /// generates it based on the given `pose_distr` which is in the form (angle distribution, (x distribution, y distribution))
    pub fn from_distributions<T, U>(
        pose_distr: (T, (T, T)),
        max_particle_count: usize,
        map: Arc<Map2D>,
        sensor_poses: Vec<Pose>,
        weight_from_error: Box<dyn FnMut(&f64) -> f64 + Send + Sync>,
        resampling_noise: Pose,
    ) -> Self
    where
        T: Distribution<U>,
        U: Into<f64>,
    {
        let (angle_distr, (x_distr, y_distr)) = pose_distr;
        let mut belief = Vec::with_capacity(max_particle_count);
        for ((x, y), angle) in x_distr
            .sample_iter(&mut thread_rng())
            .zip(y_distr.sample_iter(&mut thread_rng()))
            .zip(angle_distr.sample_iter(&mut thread_rng()))
            .take(max_particle_count)
        {
            belief.push(Pose {
                angle: angle.into(),
                position: Point {
                    x: x.into(),
                    y: y.into(),
                },
            });
        }
        Self {
            max_particle_count,
            weight_sum_threshold: max_particle_count as f64 / 50., // TODO: fixed parameter
            map,
            sensor_poses,
            weight_from_error,
            belief,
            resampling_noise,
        }
    }

    /// Takes in the total change in pose sensed by motion sensors since the last update
    pub fn control_update<U: Sensor<Pose>>(&mut self, u: &U) {
        for i in 0..self.belief.len() {
            self.belief[i] += u.sense();
        }
    }

    /// Takes in a vector of ranges indexed synchronously with `self.sensor_poses`
    pub fn observation_update<Z>(&mut self, z: &[Z])
    where
        Z: Sensor<Option<f64>> + LimitedSensor<f64, Option<f64>>,
    {
        let mut errors: Vec<f64> = Vec::with_capacity(self.belief.len());
        for sample in &self.belief {
            let mut sum_error = 0.;
            for sensor in z.iter() {
                let pred_observation = self.map.raycast(*sample + sensor.get_relative_pose());
                sum_error += match sensor.sense() {
                    Some(real_dist) => match pred_observation {
                        Some(pred) => {
                            let pred_dist = pred.dist(sample.position);
                            if pred_dist <= sensor.range().unwrap_or(std::f64::MAX) {
                                (real_dist - pred_dist).abs() // powi(2) // TODO: fixed parameter
                            } else {
                                0.
                            }
                        }
                        None => 5., // TODO: fixed parameter
                    },
                    None => match pred_observation {
                        Some(_) => 5., // TODO: fixed parameter
                        None => 0.,
                    },
                };
            }
            errors.push(sum_error / z.len() as f64);
        }

        let mut new_particles = Vec::new();
        let mut rng = thread_rng();
        #[allow(clippy::float_cmp)]
        let weights: Vec<f64> = if errors.iter().all(|error| error == &0.) {
            errors
                .iter()
                .map(|_| 2. * self.weight_sum_threshold / self.belief.len() as f64) // TODO: fixed parameter
                .collect()
        } else {
            errors
                .iter()
                .map(|error| (self.weight_from_error)(error))
                .collect()
        };
        let distr = WeightedIndex::new(weights.clone()).unwrap();
        let mut sum_weights = 0.;
        // TODO: rather than have max particle count and weight sum threshold parameters,
        // it might be beneficial to use some dynamic combination of the two as the break condition.
        while sum_weights < self.weight_sum_threshold
            && new_particles.len() < self.max_particle_count
        {
            let idx = distr.sample(&mut rng);
            sum_weights += weights[idx];
            new_particles.push(self.belief[idx] + Pose::random_from_range(self.resampling_noise));
        }
        self.belief = new_particles;
    }

    pub fn get_prediction(&self) -> Pose {
        let mut average_pose = Pose::default();
        for sample in &self.belief {
            average_pose += *sample;
        }
        average_pose / (self.belief.len() as f64)
    }
}
