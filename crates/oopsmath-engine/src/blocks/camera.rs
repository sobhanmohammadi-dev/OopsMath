//! Orbit / pan / zoom camera used by the development preview.
//!
//! RMB drag orbits, MMB drag pans, wheel zooms. The system only runs while the
//! preview state is active (see `debug_scene`), so it never consumes input in
//! other states. The cursor is not grabbed or hidden.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;

pub const MIN_PITCH: f32 = 0.05;
pub const MAX_PITCH: f32 = 1.50;

#[derive(Component, Clone, Debug)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    /// Elevation above the horizon in radians, clamped to `MIN_PITCH..=MAX_PITCH`.
    pub pitch: f32,
    pub distance: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    /// Radians per pixel.
    pub orbit_speed: f32,
    /// World units per pixel, per unit of distance.
    pub pan_speed: f32,
    /// Exponential zoom rate per scroll line.
    pub zoom_speed: f32,
}

impl OrbitCamera {
    pub fn new(target: Vec3, distance: f32, yaw: f32, pitch: f32) -> Self {
        let mut cam = Self {
            target,
            yaw,
            pitch,
            distance,
            min_distance: 1.0,
            max_distance: 60.0,
            orbit_speed: 0.005,
            pan_speed: 0.0015,
            zoom_speed: 0.1,
        };
        cam.clamp();
        cam
    }

    pub fn clamp(&mut self) {
        self.pitch = self.pitch.clamp(MIN_PITCH, MAX_PITCH);
        self.distance = self.distance.clamp(self.min_distance, self.max_distance);
    }

    pub fn transform(&self) -> Transform {
        let rotation = Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(-self.pitch);
        Transform::from_translation(self.target + rotation * Vec3::Z * self.distance)
            .with_rotation(rotation)
    }
}

pub fn orbit_camera_system(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut cameras: Query<(&mut OrbitCamera, &mut Transform)>,
) {
    let delta = motion.delta;
    let orbiting = buttons.pressed(MouseButton::Right) && delta != Vec2::ZERO;
    let panning = buttons.pressed(MouseButton::Middle) && delta != Vec2::ZERO;
    let zooming = scroll.delta.y != 0.0;
    if !(orbiting || panning || zooming) {
        return;
    }
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y * 0.02,
    };

    for (mut cam, mut transform) in &mut cameras {
        if orbiting {
            cam.yaw -= delta.x * cam.orbit_speed;
            cam.pitch += delta.y * cam.orbit_speed;
        }
        if panning {
            let right = transform.rotation * Vec3::X;
            let up = transform.rotation * Vec3::Y;
            let scale = cam.pan_speed * cam.distance;
            cam.target += (-right * delta.x + up * delta.y) * scale;
        }
        if zooming {
            cam.distance *= (-lines * cam.zoom_speed).exp();
        }
        cam.clamp();
        *transform = cam.transform();
    }
}
