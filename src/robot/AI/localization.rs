use crate::robot::map::Map2D;
use crate::robot::sensors::dummy::DummyMotionSensor;
use crate::robot::sensors::LimitedSensor;
use crate::robot::sensors::Sensor;
use crate::utility::{NewPose, Point, Pose};
use nalgebra::{
    ArrayStorage, ComplexField, Matrix, Matrix1, Matrix1x6, Matrix6, Matrix6x1, RowVector1,
    RowVector6, SymmetricEigen, Vector6, U1, U13, U6,
};
use rand::distributions::WeightedIndex;
use rand::prelude::*;
use std::f64::consts::PI;
use std::ops::Range;
use std::sync::Arc;

// TODO: c o d e d u p l i c a t i o n
type Matrix13x6 = Matrix<f64, U13, U6, ArrayStorage<f64, U13, U6>>;
type Matrix13x1 = Matrix<f64, U13, U1, ArrayStorage<f64, U13, U1>>;
struct NewPoseBelief {}

impl NewPoseBelief {
    fn new(max_particle_count: usize, max_position: Point) -> Vec<NewPose> {
        let mut belief = Vec::with_capacity(max_particle_count);
        for _ in 0..max_particle_count {
            belief.push(NewPose::random(
                0.0..2. * PI,
                0.0..max_position.x,
                0.0..max_position.y,
                0.0..0.0,
                0.0..0.0,
                0.0..0.0,
            ));
        }
        belief
    }

    fn from_distributions<T, U: Clone>(
        max_particle_count: usize,
        NewPose_distr: (T, (T, T)),
    ) -> Vec<NewPose>
    where
        T: Distribution<U>,
        U: Into<f64>,
    {
        let (angle_distr, (x_distr, y_distr)) = NewPose_distr;
        let mut belief = Vec::with_capacity(max_particle_count);
        for ((x, y), angle) in x_distr
            .sample_iter(&mut thread_rng())
            .zip(y_distr.sample_iter(&mut thread_rng()))
            .zip(angle_distr.sample_iter(&mut thread_rng()))
            .take(max_particle_count)
        {
            belief.push(NewPose {
                angle: angle.clone().into(),
                position: Point {
                    x: x.clone().into(),
                    y: y.clone().into(),
                },
                vel_angle: angle.into(),
                velocity: Point {
                    x: x.clone().into(),
                    y: y.clone().into(),
                },
            });
        }
        belief
    }
}

/// Uses Unscented Kalman Filter to approximate robot NewPose
pub struct KalmanFilter<T, U>
where
    T: Sensor<f64>,
    U: Sensor<(Pose, Pose)>,
{
    pub covariance_matrix: Matrix6<f64>,
    distance_sensor: T,
    motion_sensor: U,
    pub known_state: RowVector6<f64>,
    pub real_state: RowVector6<f64>,
    sigma_matrix: Matrix13x6,
    q: Matrix6<f64>,
    r: Matrix1<f64>,
    sensor_sigma_matrix: Matrix13x1,
    beta: f64,
    alpha: f64,
    kappa: f64,
}

impl<T, U> KalmanFilter<T, U>
where
    T: Sensor<f64>,
    U: Sensor<(Pose, Pose)>,
{
    pub fn new(
        covariance_matrix: Matrix6<f64>,
        init_state: RowVector6<f64>, // the components of this vector are x-pos, y-pos, theta, x-acceleration, y-acceleration
        real_state: RowVector6<f64>,
        alpha: f64,
        kappa: f64,
        beta: f64,
        q: Matrix6<f64>,
        r: Matrix1<f64>,
        distance_sensor: T,
        motion_sensor: U,
    ) -> Self {
        Self {
            covariance_matrix,
            distance_sensor,
            motion_sensor,
            known_state: init_state,
            real_state,
            sigma_matrix: Matrix13x6::from_element(0.),
            q,
            r,
            sensor_sigma_matrix: Matrix13x1::from_element(0.),
            beta,
            alpha,
            kappa,
        }
    }

    fn gen_sigma_matrix(&mut self) {
        let mut rows: Vec<RowVector6<f64>> = Vec::new();
        rows.push(self.known_state);
        let lambda = (self.alpha.powi(2)) * (6. + self.kappa) - 6.;
        let eigendecomp = (self
            .covariance_matrix
            .map(|e| (e * 100000.).round() / 100000.)
            * (6. + lambda))
            .symmetric_eigen();
        let mut diagonalization = eigendecomp.eigenvalues;
        diagonalization.data.iter_mut().for_each(|e| {
            *e = e.max(0.).sqrt();
        });
        let square_root_cov = eigendecomp.eigenvectors
            * Matrix6::from_diagonal(&diagonalization)
            * eigendecomp.eigenvectors.try_inverse().unwrap();
        for i in 0..=(6 - 1) {
            rows.push(self.known_state + square_root_cov.row(i));
        }
        for i in 0..=(6 - 1) {
            rows.push(self.known_state - square_root_cov.row(i));
        }

        self.sigma_matrix = Matrix13x6::from_rows(rows.as_slice());
    }

    pub fn prediction_update(&mut self, time: f64) {
        self.gen_sigma_matrix();
        let control = self.motion_sensor.sense();
        let temp: NewPose;
        self.real_state[0] = (self.real_state[0] + self.real_state[3] * time) % (2. * PI);
        self.real_state[1] = self.real_state[1] + self.real_state[4] * time;
        self.real_state[2] = self.real_state[2] + self.real_state[5] * time;
        self.real_state[3] += control.0.angle * time;
        self.real_state[4] += control.0.position.x * time;
        self.real_state[5] += control.0.position.y * time;
        temp = self.real_state.clone().into();
        self.real_state = temp
            .clamp_control_update(Range {
                start: Point { x: 0., y: 0. },
                end: Point { x: 200., y: 200. },
            })
            .into();
        self.sigma_matrix.row_iter_mut().for_each(|mut e| {
            e[0] += e[3] * time;
            e[1] += e[4] * time;
            e[2] += e[5] * time;
            e[3] += control.1.angle * time;
            e[4] += control.1.position.x * time;
            e[5] += control.1.position.y * time;

            let temp_point = (Point { x: e[1], y: e[2] })
                .clamp(Point { x: 0., y: 0. }, Point { x: 200., y: 200. });
            if temp_point.x != e[1] {
                e[1] = temp_point.x;
                e[4] = 0.;
            }

            if temp_point.y != e[2] {
                e[2] = temp_point.y;
                e[5] = 0.;
            }
        });
        let lambda = (self.alpha.powi(2)) * (6. + self.kappa) - 6.;
        self.known_state = RowVector6::from_element(0.);
        for i in 0..=(2 * 6) {
            self.known_state +=
                self.sigma_matrix.row(i) / (6. + lambda) * if i != 0 { 1. / 2. } else { lambda };
        }
        let temp_sigma_matrix =
            self.sigma_matrix - Matrix13x6::from_rows(&vec![self.known_state; 13]);
        self.covariance_matrix = Matrix6::from_element(0.);
        for i in 0..=(2 * 6) {
            self.covariance_matrix += temp_sigma_matrix.row(i).transpose()
                * temp_sigma_matrix.row(i)
                * if i == 0 {
                    lambda / (6. + lambda) + (1. - self.alpha.powi(2) + self.beta)
                } else {
                    1. / (2. * (6. + lambda))
                };
        }
        self.covariance_matrix += self.q;
    }

    pub fn measurement_update(&mut self, sensor_update: RowVector1<f64>) {
        let mut sensor_data: Vec<f64> = Vec::new();
        self.sensor_sigma_matrix = Matrix13x1::from_rows(
            self.sigma_matrix
                .row_iter()
                .map(|e| {
                    sensor_data = vec![self.distance_sensor.sense_from_pose(NewPose {
                        angle: e[0],
                        position: Point { x: e[1], y: e[2] },
                        vel_angle: e[3],
                        velocity: Point { x: e[4], y: e[5] },
                    })];

                    RowVector1::from_vec(sensor_data.clone())
                })
                .collect::<Vec<_>>()
                .as_slice(),
        );
        let lambda = (self.alpha.powi(2)) * (6. + self.kappa) - 6.;
        let mut sensor_predicted = RowVector1::from_element(0.);
        for i in 0..=(2 * 6) {
            sensor_predicted += self.sensor_sigma_matrix.row(i) / (6. + lambda)
                * if i != 0 { 1. / 2. } else { lambda };
        }

        let mut cov_zz = Matrix1::from_element(0.);
        let temp_sensor_sigma_matrix =
            self.sensor_sigma_matrix - Matrix13x1::from_rows(&vec![sensor_predicted; 13]);
        for i in 0..=(2 * 6) {
            cov_zz += temp_sensor_sigma_matrix.row(i).transpose()
                * temp_sensor_sigma_matrix.row(i)
                * if i == 0 {
                    lambda / (6. + lambda) + (1. - self.alpha.powi(2) + self.beta)
                } else {
                    1. / (2. * (6. + lambda))
                };
        }
        cov_zz += self.r;

        let mut cov_xz = Matrix6x1::from_element(0.);
        let temp_sigma_matrix =
            self.sigma_matrix - Matrix13x6::from_rows(&vec![self.known_state; 13]);
        for i in 0..=(2 * 6) {
            cov_xz += temp_sigma_matrix.row(i).transpose()
                * temp_sensor_sigma_matrix.row(i)
                * if i == 0 {
                    lambda / (6. + lambda) + (1. - self.alpha.powi(2) + self.beta)
                } else {
                    1. / (2. * (6. + lambda))
                };
        }
        let k = cov_xz * cov_zz.try_inverse().unwrap();
        self.known_state += (k * (sensor_update - sensor_predicted).transpose()).transpose();
        self.covariance_matrix -= k * cov_zz * k.transpose();
    }
}

/// A NewPose localizer that uses likelyhood-based Monte Carlo Localization
/// and takes in motion and range finder sensor data
pub struct DistanceFinderMCL {
    pub map: Arc<Map2D>,
    pub belief: Vec<NewPose>,
    max_particle_count: usize,
    weight_sum_threshold: f64,
    weight_from_error: Box<dyn FnMut(&f64) -> f64 + Send + Sync>,
    resampling_noise: NewPose,
}

impl DistanceFinderMCL {
    /// Generates a new localizer with the given parameters.
    /// Every step, the localizer should recieve a control and observation update
    pub fn new(
        max_particle_count: usize,
        map: Arc<Map2D>,
        weight_from_error: Box<dyn FnMut(&f64) -> f64 + Send + Sync>,
        resampling_noise: NewPose,
    ) -> Self {
        let max_position = (map.width, map.height);
        let belief = NewPoseBelief::new(max_particle_count, max_position.into());
        Self {
            max_particle_count,
            weight_sum_threshold: max_particle_count as f64 / 60., // TODO: fixed parameter
            map,
            weight_from_error,
            belief,
            resampling_noise,
        }
    }

    /// Similar to new, but instead of generating `belief` based on a uniform distribution,
    /// generates it based on the given `pose_distr` which is in the form (angle distribution, (x distribution, y distribution))
    pub fn from_distributions<T, U: Clone>(
        NewPose_distr: (T, (T, T)),
        max_particle_count: usize,
        map: Arc<Map2D>,
        weight_from_error: Box<dyn FnMut(&f64) -> f64 + Send + Sync>,
        resampling_noise: NewPose,
    ) -> Self
    where
        T: Distribution<U>,
        U: Into<f64>,
    {
        let belief = NewPoseBelief::from_distributions(max_particle_count, NewPose_distr);
        Self {
            max_particle_count,
            weight_sum_threshold: max_particle_count as f64 / 60., // TODO: fixed parameter
            map,
            weight_from_error,
            belief,
            resampling_noise,
        }
    }

    /// Takes in a sensor which senses the total change in NewPose sensed since the last update
    pub fn control_update<U: Sensor<NewPose>>(&mut self, u: &U) {
        let update = u.sense();
        self.belief.iter_mut().for_each(|p| *p += update);
    }

    /// Takes in a vector of distance finder sensors (e.g. laser range finder)
    pub fn observation_update<Z>(&mut self, z: &[Z])
    where
        Z: Sensor<Option<f64>> + LimitedSensor<f64, Option<f64>>,
    {
        let mut errors: Vec<f64> = Vec::with_capacity(self.belief.len());
        for sample in &self.belief {
            let mut sum_error = 0.;
            for sensor in z.iter() {
                let pred_observation = self.map.raycast(*sample + sensor.relative_pose());
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
                        None => 6., // TODO: fixed parameter
                    },
                    None => match pred_observation {
                        Some(_) => 6., // TODO: fixed parameter
                        None => 0.,
                    },
                };
            }
            errors.push(sum_error / z.len() as f64);
        }

        let mut new_particles = Vec::new();
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
        let mut rng = thread_rng();
        // TODO: rather than have max particle count and weight sum threshold parameters,
        // it might be beneficial to use some dynamic combination of the two as the break condition.
        while sum_weights < self.weight_sum_threshold
            && new_particles.len() < self.max_particle_count
        {
            let idx = distr.sample(&mut rng);
            sum_weights += weights[idx];
            new_particles
                .push(self.belief[idx] + NewPose::random_from_range(self.resampling_noise));
        }
        self.belief = new_particles;
    }

    pub fn get_prediction(&self) -> NewPose {
        let mut average_pose = NewPose::default();
        for sample in &self.belief {
            average_pose += *sample;
        }
        average_pose / (self.belief.len() as f64)
    }
}

/// A NewPose localizer that uses likelyhood-based Monte Carlo Localization
/// and takes in motion and range finder sensor data
pub struct ObjectDetectorMCL {
    pub map: Arc<Map2D>,
    pub belief: Vec<NewPose>,
    max_particle_count: usize,
    weight_sum_threshold: f64,
    weight_from_error: Box<dyn FnMut(&f64) -> f64 + Send + Sync>,
    resampling_noise: NewPose,
}

impl ObjectDetectorMCL {
    /// Generates a new localizer with the given parameters.
    /// Every step, the localizer should recieve a control and observation update
    pub fn new(
        max_particle_count: usize,
        map: Arc<Map2D>,
        weight_from_error: Box<dyn FnMut(&f64) -> f64 + Send + Sync>,
        resampling_noise: NewPose,
    ) -> Self {
        let max_position = (map.width, map.height);
        let belief = NewPoseBelief::new(max_particle_count, max_position.into());
        Self {
            max_particle_count,
            weight_sum_threshold: max_particle_count as f64 / 60., // TODO: fixed parameter
            map,
            weight_from_error,
            belief,
            resampling_noise,
        }
    }

    /// Similar to new, but instead of generating `belief` based on a uniform distribution,
    /// generates it based on the given `pose_distr` which is in the form (angle distribution, (x distribution, y distribution))
    pub fn from_distributions<T, U: Clone>(
        NewPose_distr: (T, (T, T)),
        max_particle_count: usize,
        map: Arc<Map2D>,
        weight_from_error: Box<dyn FnMut(&f64) -> f64 + Send + Sync>,
        resampling_noise: NewPose,
    ) -> Self
    where
        T: Distribution<U>,
        U: Into<f64>,
    {
        let belief = NewPoseBelief::from_distributions(max_particle_count, NewPose_distr);
        Self {
            max_particle_count,
            weight_sum_threshold: max_particle_count as f64 / 60., // TODO: fixed parameter
            map,
            weight_from_error,
            belief,
            resampling_noise,
        }
    }

    /// Takes in a sensor which senses the total change in NewPose since the last update
    pub fn control_update<U: Sensor<NewPose>>(&mut self, u: &U) {
        let update = u.sense();
        self.belief.iter_mut().for_each(|p| *p += update);
    }

    /// Takes in a sensor which senses all objects within a certain field of view
    pub fn observation_update<Z>(&mut self, z: &Z)
    where
        Z: Sensor<Vec<Point>> + LimitedSensor<f64, Vec<Point>>,
    {
        let observation = {
            let mut observation = z.sense();
            observation.sort_by(|a, b| a.mag().partial_cmp(&b.mag()).unwrap());
            observation
        };
        let fov = if let Some(range) = z.range() {
            range
        } else {
            2. * PI
        };
        let mut errors: Vec<f64> = Vec::with_capacity(self.belief.len());
        for sample in &self.belief {
            let mut sum_error = 0.;
            let pred_observation = {
                let mut pred_observation = self.map.cull_points(*sample + z.relative_pose(), fov);
                pred_observation.sort_by(|a, b| a.mag().partial_cmp(&b.mag()).unwrap());
                pred_observation
            };
            // TODO: fixed parameter
            // This method of calculating error is not entirely sound
            let mut total = 0;
            for (real, pred) in observation.iter().zip(pred_observation.iter()) {
                sum_error += (*real - *pred).mag();
                total += 1;
            }
            // TODO: fixed parameter
            // TODO: panics at Uniform::new called with `low >= high` when erorr is divided by total
            sum_error += 6. * (observation.len() as f64 - pred_observation.len() as f64).abs();
            errors.push(sum_error);
        }

        let mut new_particles = Vec::new();
        #[allow(clippy::float_cmp)]
        let weights: Vec<f64> = if errors
            .iter()
            .all(|error| error == &0. || error == &std::f64::NAN)
        {
            errors
                .iter()
                .map(|_| self.weight_sum_threshold / self.belief.len() as f64) // TODO: fixed parameter
                .collect()
        } else {
            errors
                .iter()
                .map(|error| (self.weight_from_error)(error))
                .collect()
        };
        let distr = WeightedIndex::new(weights.clone()).unwrap();
        let mut sum_weights = 0.;
        let mut rng = thread_rng();
        // TODO: rather than have max particle count and weight sum threshold parameters,
        // it might be beneficial to use some dynamic combination of the two as the break condition.
        while sum_weights < self.weight_sum_threshold
            && new_particles.len() < self.max_particle_count
        {
            let idx = distr.sample(&mut rng);
            sum_weights += weights[idx];
            new_particles
                .push(self.belief[idx] + NewPose::random_from_range(self.resampling_noise));
        }
        self.belief = new_particles;
    }

    pub fn get_prediction(&self) -> NewPose {
        let mut average_pose = NewPose::default();
        for sample in &self.belief {
            average_pose += *sample;
        }
        average_pose / (self.belief.len() as f64)
    }
}
