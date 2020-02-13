use crate::{
    map::Map2D,
    utility::{Point, Pose},
};
use piston_window::*;
use std::{f64::consts::*, sync::Arc};

pub fn draw_map<G>(
    map: Arc<Map2D>,
    color: [f32; 4],
    point_radius: f64,
    line_radius: f64,
    scale: f64,
    offset: Point,
    transform: [[f64; 3]; 2],
    g: &mut G,
) where
    G: Graphics,
{
    let point_radius: Point = (point_radius, point_radius).into();
    for line in map.lines.clone() {
        line_from_to(
            color,
            line_radius,
            map.vertices[line.0] * scale + offset,
            map.vertices[line.1] * scale + offset,
            transform,
            g,
        );
    }
    for &point in &map.points {
        let v: Point = map.vertices[point] * scale + offset;
        ellipse_from_to(color, v + point_radius, v - point_radius, transform, g);
    }
}

pub fn point_cloud<G>(
    points: &[Point],
    color: [f32; 4],
    point_radius: f64,
    scale: f64,
    offset: Point,
    transform: [[f64; 3]; 2],
    g: &mut G,
) where
    G: Graphics,
{
    let point_radius: Point = (point_radius, point_radius).into();
    for point in points {
        let center = offset + *point * scale;
        ellipse_from_to(
            color,
            center - point_radius,
            center + point_radius,
            transform,
            g,
        );
    }
}

pub fn isoceles_triangle<G: Graphics>(
    color: [f32; 4],
    margin: Point,
    pose_scale: f64,
    triangle_scale: f64,
    pose: Pose,
    transform: math::Matrix2d,
    g: &mut G,
) {
    polygon(
        color,
        &[
            [
                pose.position.x * pose_scale + margin.x + triangle_scale * 15. * pose.angle.cos(),
                pose.position.y * pose_scale + margin.y + triangle_scale * 15. * pose.angle.sin(),
            ],
            [
                pose.position.x * pose_scale
                    + margin.x
                    + triangle_scale * 10. * (pose.angle + 2. * FRAC_PI_3).cos(),
                pose.position.y * pose_scale
                    + margin.y
                    + triangle_scale * 10. * (pose.angle + 2. * FRAC_PI_3).sin(),
            ],
            [
                pose.position.x * pose_scale
                    + margin.x
                    + triangle_scale * 10. * (pose.angle + 4. * FRAC_PI_3).cos(),
                pose.position.y * pose_scale
                    + margin.y
                    + triangle_scale * 10. * (pose.angle + 4. * FRAC_PI_3).sin(),
            ],
        ],
        transform,
        g,
    );
}
