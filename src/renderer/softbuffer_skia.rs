use std::num::NonZeroU32;
use std::sync::Arc;
use softbuffer::{Context, Surface};
use tiny_skia::{
    Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform,
};
use tracing::{error, warn};
use winit::window::Window;

use super::RenderBackend;
use crate::physics::{
    ParticleType, PetStateKind, WorldState, PET_HEIGHT, PET_WIDTH,
};

/// Software 2D Renderer using `softbuffer` and `tiny-skia` with full Windows 11 premultiplied alpha transparency.
pub struct SoftbufferSkiaRenderer {
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    pixmap: Option<Pixmap>,
    width: u32,
    height: u32,
}

impl SoftbufferSkiaRenderer {
    pub fn new(window: Arc<Window>, width: u32, height: u32) -> Result<Self, String> {
        let context = Context::new(window.clone())
            .map_err(|e| format!("Failed to create softbuffer context: {e}"))?;

        let mut surface = Surface::new(&context, window)
            .map_err(|e| format!("Failed to create softbuffer surface: {e}"))?;

        if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
            let _ = surface.resize(w, h);
        }

        let pixmap = Pixmap::new(width.max(1), height.max(1));

        Ok(Self {
            _context: context,
            surface,
            pixmap,
            width,
            height,
        })
    }
}

impl RenderBackend for SoftbufferSkiaRenderer {
    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;

        if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
            let _ = self.surface.resize(w, h);
        }

        self.pixmap = Pixmap::new(width, height);
    }

    fn render_frame(&mut self, world: &WorldState, debug_draw_platforms: bool) {
        let pixmap = match &mut self.pixmap {
            Some(p) => p,
            None => return,
        };

        // CRITICAL Transparency Rule: Clear tiny-skia buffer to completely transparent (all zeroes)
        pixmap.fill(Color::TRANSPARENT);

        // 1. Draw Debug Platform Overlays if enabled
        if debug_draw_platforms || world.debug_draw_platforms {
            Self::draw_debug_overlay(pixmap, world);
        }

        // 2. Draw Floating Particles (Hearts, Sparkles, Zzz, Treats)
        Self::draw_particles(pixmap, world);

        // 3. Draw Procedural Animated Pet
        Self::draw_animated_pet(pixmap, world);

        // 4. Draw Speech Bubble if present
        if let Some(speech) = &world.active_speech {
            Self::draw_speech_bubble(pixmap, world, &speech.text);
        }

        // 5. Blit tiny-skia RGBA premultiplied pixels to softbuffer 0xAARRGGBB format
        let mut buffer = match self.surface.buffer_mut() {
            Ok(b) => b,
            Err(e) => {
                warn!("Failed to acquire softbuffer buffer: {e}");
                return;
            }
        };

        let src_bytes = pixmap.data();
        let buffer_len = buffer.len();
        let pixel_count = (self.width as usize * self.height as usize).min(buffer_len);

        for (dst, src) in buffer[..pixel_count].iter_mut().zip(src_bytes.chunks_exact(4)) {
            let r = src[0] as u32;
            let g = src[1] as u32;
            let b = src[2] as u32;
            let a = src[3] as u32;
            *dst = (a << 24) | (r << 16) | (g << 8) | b;
        }

        if let Err(e) = buffer.present() {
            error!("Failed to present softbuffer frame: {e}");
        }
    }
}

impl SoftbufferSkiaRenderer {
    /// Renders procedural animated cyber-pet with dynamic eye blinking, walking paws, and squash/stretch physics.
    fn draw_animated_pet(pixmap: &mut Pixmap, world: &WorldState) {
        let x = world.x;
        let y = world.y;
        let t = world.anim_timer;

        let walk_cycle = (t * 12.0).sin();
        let breath = (t * 3.5).sin() * 1.5;
        let is_blinking = (t * 1.8).sin() > 0.95;

        // Dynamic squash and stretch based on vertical velocity
        let stretch_y = (world.vy / 1200.0).clamp(-0.25, 0.35);
        let eff_height = PET_HEIGHT * (1.0 + stretch_y) + breath;
        let eff_width = PET_WIDTH * (1.0 - stretch_y * 0.5);
        let center_x = x + PET_WIDTH / 2.0;
        let bottom_y = y + PET_HEIGHT;
        let top_y = bottom_y - eff_height;

        // Dynamic transform for acrobatics (backflips)
        let transform = if world.rotation_rad != 0.0 {
            let rad = world.rotation_rad;
            let cos = rad.cos();
            let sin = rad.sin();
            let cy = y + PET_HEIGHT / 2.0;
            Transform::from_row(
                cos, sin,
                -sin, cos,
                center_x - (center_x * cos - cy * sin),
                cy - (center_x * sin + cy * cos),
            )
        } else {
            Transform::identity()
        };

        let mut paint = Paint::default();
        paint.anti_alias = true;

        // A. Draw Feet / Paws
        let foot_y_offset = match world.state_kind {
            PetStateKind::Walking => walk_cycle * 3.5,
            PetStateKind::Sitting | PetStateKind::Sleeping => -2.0,
            PetStateKind::DraggedByUser | PetStateKind::Falling => 4.0,
            _ => 0.0,
        };

        // Left Foot
        paint.set_color_rgba8(20, 180, 230, 255);
        let left_foot = Rect::from_xywh(
            center_x - 18.0,
            bottom_y - 8.0 + foot_y_offset,
            12.0,
            8.0,
        );
        if let Some(r) = left_foot {
            let mut pb = PathBuilder::new();
            pb.push_rect(r);
            if let Some(path) = pb.finish() {
                pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
            }
        }

        // Right Foot
        let right_foot = Rect::from_xywh(
            center_x + 6.0,
            bottom_y - 8.0 - foot_y_offset,
            12.0,
            8.0,
        );
        if let Some(r) = right_foot {
            let mut pb = PathBuilder::new();
            pb.push_rect(r);
            if let Some(path) = pb.finish() {
                pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
            }
        }

        // B. Draw Body (Cute Rounded Cyber-Chibi Shape)
        let body_rect = Rect::from_xywh(
            center_x - eff_width / 2.0,
            top_y,
            eff_width,
            eff_height - 4.0,
        );

        if let Some(r) = body_rect {
            let mut pb = PathBuilder::new();
            let radius = 18.0;
            pb.move_to(r.left() + radius, r.top());
            pb.line_to(r.right() - radius, r.top());
            pb.quad_to(r.right(), r.top(), r.right(), r.top() + radius);
            pb.line_to(r.right(), r.bottom() - radius);
            pb.quad_to(r.right(), r.bottom(), r.right() - radius, r.bottom());
            pb.line_to(r.left() + radius, r.bottom());
            pb.quad_to(r.left(), r.bottom(), r.left(), r.bottom() - radius);
            pb.line_to(r.left(), r.top() + radius);
            pb.quad_to(r.left(), r.top(), r.left() + radius, r.top());
            pb.close();

            if let Some(path) = pb.finish() {
                // Vibrant Cyan / Neon Turquoise body
                paint.set_color_rgba8(30, 220, 245, 245);
                pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);

                let mut stroke = Stroke::default();
                stroke.width = 2.5;
                paint.set_color_rgba8(10, 80, 120, 255);
                pixmap.stroke_path(&path, &paint, &stroke, transform, None);
            }
        }

        // C. Draw Cute Ears / Horns
        let ear_wiggle = (t * 8.0).sin() * 2.0;
        let mut pb_ear = PathBuilder::new();
        pb_ear.move_to(center_x - 16.0, top_y + 4.0);
        pb_ear.line_to(center_x - 12.0 + ear_wiggle, top_y - 12.0);
        pb_ear.line_to(center_x - 4.0, top_y + 2.0);
        pb_ear.close();

        pb_ear.move_to(center_x + 4.0, top_y + 2.0);
        pb_ear.line_to(center_x + 12.0 - ear_wiggle, top_y - 12.0);
        pb_ear.line_to(center_x + 16.0, top_y + 4.0);
        pb_ear.close();

        if let Some(ear_path) = pb_ear.finish() {
            paint.set_color_rgba8(255, 80, 160, 240); // Neon Pink inner ears
            pixmap.fill_path(&ear_path, &paint, FillRule::Winding, transform, None);
        }

        // D. Draw Eyes & Gaze Direction (Tracks Cursor)
        let eye_y = top_y + eff_height * 0.42;
        let (cursor_dx, cursor_dy) = if let Some((cx, cy)) = world.cursor_pos {
            let dx = ((cx - center_x) / 100.0).clamp(-2.5, 2.5);
            let dy = ((cy - eye_y) / 100.0).clamp(-2.0, 2.0);
            (dx, dy)
        } else {
            let dx = if world.facing_right { 1.5 } else { -1.5 };
            (dx, 0.0)
        };

        if is_blinking || world.state_kind == PetStateKind::Sitting || world.state_kind == PetStateKind::Sleeping || world.state_kind == PetStateKind::Petted {
            // Happy closed eyes (^ ^)
            let mut stroke = Stroke::default();
            stroke.width = 2.5;
            paint.set_color_rgba8(15, 30, 50, 255);

            let mut pb_eyes = PathBuilder::new();
            pb_eyes.move_to(center_x - 14.0, eye_y);
            pb_eyes.quad_to(center_x - 10.0, eye_y - 5.0, center_x - 6.0, eye_y);

            pb_eyes.move_to(center_x + 6.0, eye_y);
            pb_eyes.quad_to(center_x + 10.0, eye_y - 5.0, center_x + 14.0, eye_y);

            if let Some(path) = pb_eyes.finish() {
                pixmap.stroke_path(&path, &paint, &stroke, transform, None);
            }
        } else {
            // Expressive Open Eyes
            let eye_h = if world.state_kind == PetStateKind::DraggedByUser { 12.0 } else { 9.0 };
            let eye_w = if world.state_kind == PetStateKind::DraggedByUser { 8.0 } else { 6.0 };

            // Eye whites
            paint.set_color_rgba8(255, 255, 255, 255);
            let left_eye = Rect::from_xywh(center_x - 13.0, eye_y - eye_h / 2.0, eye_w, eye_h);
            let right_eye = Rect::from_xywh(center_x + 7.0, eye_y - eye_h / 2.0, eye_w, eye_h);

            for eye_rect in [left_eye, right_eye].into_iter().flatten() {
                let mut pb = PathBuilder::new();
                pb.push_rect(eye_rect);
                if let Some(path) = pb.finish() {
                    pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
                }
            }

            // Pupils with dynamic cursor tracking
            paint.set_color_rgba8(15, 25, 45, 255);
            let left_pupil = Rect::from_xywh(center_x - 11.5 + cursor_dx, eye_y - 2.5 + cursor_dy, 3.5, 5.0);
            let right_pupil = Rect::from_xywh(center_x + 8.5 + cursor_dx, eye_y - 2.5 + cursor_dy, 3.5, 5.0);

            for pupil_rect in [left_pupil, right_pupil].into_iter().flatten() {
                let mut pb = PathBuilder::new();
                pb.push_rect(pupil_rect);
                if let Some(path) = pb.finish() {
                    pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
                }
            }
        }

        // E. Cute Blush Cheeks
        let blush_alpha = if world.state_kind == PetStateKind::Petted { 240 } else { 180 };
        paint.set_color_rgba8(255, 100, 160, blush_alpha);
        let left_cheek = Rect::from_xywh(center_x - 17.0, eye_y + 5.0, 5.0, 3.0);
        let right_cheek = Rect::from_xywh(center_x + 12.0, eye_y + 5.0, 5.0, 3.0);
        for cheek in [left_cheek, right_cheek].into_iter().flatten() {
            let mut pb = PathBuilder::new();
            pb.push_rect(cheek);
            if let Some(path) = pb.finish() {
                pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
            }
        }

        // F. Cute Eating Animation Mouth
        if world.state_kind == PetStateKind::Eating {
            let chew = ((t * 16.0).sin() * 2.5).abs();
            let mut pb_mouth = PathBuilder::new();
            pb_mouth.move_to(center_x - 4.0, eye_y + 6.0);
            pb_mouth.quad_to(center_x, eye_y + 6.0 + chew, center_x + 4.0, eye_y + 6.0);
            if let Some(mp) = pb_mouth.finish() {
                let mut stroke = Stroke::default();
                stroke.width = 2.0;
                paint.set_color_rgba8(200, 40, 80, 255);
                pixmap.stroke_path(&mp, &paint, &stroke, transform, None);
            }
        }
    }

    /// Renders floating particles: Hearts, Sparkles, Zzz, Treats
    fn draw_particles(pixmap: &mut Pixmap, world: &WorldState) {
        let mut paint = Paint::default();
        paint.anti_alias = true;

        for p in &world.particles {
            let alpha_ratio = (p.lifetime / p.max_lifetime).clamp(0.0, 1.0);
            let alpha = (alpha_ratio * 255.0) as u8;

            match p.particle_type {
                ParticleType::Heart => {
                    paint.set_color_rgba8(255, 50, 120, alpha);
                    let mut pb = PathBuilder::new();
                    let s = p.size;
                    pb.move_to(p.x, p.y);
                    pb.cubic_to(p.x - s / 2.0, p.y - s / 2.0, p.x - s, p.y + s / 3.0, p.x, p.y + s);
                    pb.cubic_to(p.x + s, p.y + s / 3.0, p.x + s / 2.0, p.y - s / 2.0, p.x, p.y);
                    pb.close();
                    if let Some(path) = pb.finish() {
                        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
                    }
                }

                ParticleType::Sparkle { r, g, b } => {
                    paint.set_color_rgba8(r, g, b, alpha);
                    let mut pb = PathBuilder::new();
                    let s = p.size;
                    pb.move_to(p.x, p.y - s);
                    pb.line_to(p.x + s * 0.3, p.y);
                    pb.line_to(p.x + s, p.y);
                    pb.line_to(p.x + s * 0.3, p.y + s * 0.3);
                    pb.line_to(p.x, p.y + s);
                    pb.line_to(p.x - s * 0.3, p.y + s * 0.3);
                    pb.line_to(p.x - s, p.y);
                    pb.line_to(p.x - s * 0.3, p.y);
                    pb.close();
                    if let Some(path) = pb.finish() {
                        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
                    }
                }

                ParticleType::Zzz => {
                    let mut stroke = Stroke::default();
                    stroke.width = 2.0;
                    paint.set_color_rgba8(160, 210, 255, alpha);

                    let mut pb = PathBuilder::new();
                    let s = p.size;
                    pb.move_to(p.x - s / 2.0, p.y - s / 2.0);
                    pb.line_to(p.x + s / 2.0, p.y - s / 2.0);
                    pb.line_to(p.x - s / 2.0, p.y + s / 2.0);
                    pb.line_to(p.x + s / 2.0, p.y + s / 2.0);

                    if let Some(path) = pb.finish() {
                        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                    }
                }

                ParticleType::Treat => {
                    // Draw a strawberry/berry treat!
                    paint.set_color_rgba8(255, 60, 90, 240);
                    let mut pb = PathBuilder::new();
                    let s = p.size;
                    pb.move_to(p.x, p.y - s / 2.0);
                    pb.cubic_to(p.x + s / 2.0, p.y - s / 2.0, p.x + s / 2.0, p.y + s / 2.0, p.x, p.y + s);
                    pb.cubic_to(p.x - s / 2.0, p.y + s / 2.0, p.x - s / 2.0, p.y - s / 2.0, p.x, p.y - s / 2.0);
                    pb.close();
                    if let Some(path) = pb.finish() {
                        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
                    }

                    // Green leaf on top
                    paint.set_color_rgba8(50, 220, 80, 240);
                    let mut pb_leaf = PathBuilder::new();
                    pb_leaf.move_to(p.x, p.y - s / 2.0);
                    pb_leaf.line_to(p.x - 4.0, p.y - s / 2.0 - 4.0);
                    pb_leaf.line_to(p.x + 4.0, p.y - s / 2.0 - 4.0);
                    pb_leaf.close();
                    if let Some(lp) = pb_leaf.finish() {
                        pixmap.fill_path(&lp, &paint, FillRule::Winding, Transform::identity(), None);
                    }
                }
            }
        }
    }

    /// Renders a high-contrast speech bubble above the pet's head.
    fn draw_speech_bubble(pixmap: &mut Pixmap, world: &WorldState, text: &str) {
        let center_x = world.x + PET_WIDTH / 2.0;
        let bubble_bottom = world.y - 12.0;

        let char_count = text.len().max(8);
        let bubble_width = (char_count as f32 * 7.5 + 24.0).clamp(90.0, 280.0);
        let bubble_height = 28.0;

        let bubble_left = (center_x - bubble_width / 2.0).clamp(10.0, world.screen_width - bubble_width - 10.0);
        let bubble_top = bubble_bottom - bubble_height;

        let mut pb = PathBuilder::new();
        let r = 8.0;
        pb.move_to(bubble_left + r, bubble_top);
        pb.line_to(bubble_left + bubble_width - r, bubble_top);
        pb.quad_to(bubble_left + bubble_width, bubble_top, bubble_left + bubble_width, bubble_top + r);
        pb.line_to(bubble_left + bubble_width, bubble_top + bubble_height - r);
        pb.quad_to(bubble_left + bubble_width, bubble_top + bubble_height, bubble_left + bubble_width - r, bubble_top + bubble_height);

        let ptr_x = center_x.clamp(bubble_left + 16.0, bubble_left + bubble_width - 16.0);
        pb.line_to(ptr_x + 6.0, bubble_top + bubble_height);
        pb.line_to(ptr_x, bubble_bottom);
        pb.line_to(ptr_x - 6.0, bubble_top + bubble_height);

        pb.line_to(bubble_left + r, bubble_top + bubble_height);
        pb.quad_to(bubble_left, bubble_top + bubble_height, bubble_left, bubble_top + bubble_height - r);
        pb.line_to(bubble_left, bubble_top + r);
        pb.quad_to(bubble_left, bubble_top, bubble_left + r, bubble_top);
        pb.close();

        if let Some(bubble_path) = pb.finish() {
            let mut paint = Paint::default();
            paint.anti_alias = true;

            paint.set_color_rgba8(250, 252, 255, 240);
            pixmap.fill_path(&bubble_path, &paint, FillRule::Winding, Transform::identity(), None);

            let mut stroke = Stroke::default();
            stroke.width = 1.5;
            paint.set_color_rgba8(30, 41, 59, 230);
            pixmap.stroke_path(&bubble_path, &paint, &stroke, Transform::identity(), None);

            paint.set_color_rgba8(15, 23, 42, 220);
            let text_rect = Rect::from_xywh(bubble_left + 10.0, bubble_top + 8.0, bubble_width - 20.0, bubble_height - 16.0);
            if let Some(tr) = text_rect {
                let line_y = tr.top() + tr.height() / 2.0 - 1.0;
                let mut pb_text = PathBuilder::new();
                pb_text.move_to(tr.left() + 4.0, line_y);
                pb_text.line_to(tr.right() - 4.0, line_y);
                if let Some(tp) = pb_text.finish() {
                    let mut s = Stroke::default();
                    s.width = 3.0;
                    pixmap.stroke_path(&tp, &paint, &s, Transform::identity(), None);
                }
            }
        }
    }

    /// Renders neon wireframe debug overlay showing scanned windows, platform colliders, and pet hitbox.
    fn draw_debug_overlay(pixmap: &mut Pixmap, world: &WorldState) {
        let mut paint = Paint::default();
        paint.anti_alias = true;

        // 1. Draw Scanned Windows (Neon Cyan Outlines)
        let mut stroke_win = Stroke::default();
        stroke_win.width = 1.5;
        paint.set_color_rgba8(0, 220, 255, 140);

        for win in &world.scanned_windows {
            // Highlight fullscreen windows in deep amber/orange
            if win.is_fullscreen {
                paint.set_color_rgba8(255, 140, 0, 100);
            } else {
                paint.set_color_rgba8(0, 220, 255, 140);
            }

            let rect = Rect::from_xywh(
                win.bounds.left as f32,
                win.bounds.top as f32,
                win.bounds.width() as f32,
                win.bounds.height() as f32,
            );
            if let Some(r) = rect {
                let mut pb = PathBuilder::new();
                pb.push_rect(r);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &paint, &stroke_win, Transform::identity(), None);
                }
            }
        }

        // 2. Draw Walkable Platforms (Bright Neon Lime/Green Segments)
        let mut stroke_plat = Stroke::default();
        stroke_plat.width = 3.5;
        paint.set_color_rgba8(57, 255, 20, 240);

        for plat in &world.platforms {
            let mut pb = PathBuilder::new();
            pb.move_to(plat.x1, plat.y);
            pb.line_to(plat.x2, plat.y);

            pb.move_to(plat.x1, plat.y - 4.0);
            pb.line_to(plat.x1, plat.y + 4.0);
            pb.move_to(plat.x2, plat.y - 4.0);
            pb.line_to(plat.x2, plat.y + 4.0);

            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke_plat, Transform::identity(), None);
            }
        }

        // 3. Draw Pet Bounding Box & Foot Anchor
        let mut stroke_pet = Stroke::default();
        stroke_pet.width = 2.0;
        paint.set_color_rgba8(255, 0, 128, 220);

        if let Some(r) = Rect::from_xywh(world.x, world.y, PET_WIDTH, PET_HEIGHT) {
            let mut pb = PathBuilder::new();
            pb.push_rect(r);
            let feet_x = world.x + PET_WIDTH / 2.0;
            let feet_y = world.y + PET_HEIGHT;
            pb.move_to(feet_x - 5.0, feet_y);
            pb.line_to(feet_x + 5.0, feet_y);
            pb.move_to(feet_x, feet_y - 5.0);
            pb.line_to(feet_x, feet_y + 5.0);

            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke_pet, Transform::identity(), None);
            }
        }

        // 4. Draw Cursor Crosshair
        if let Some((cx, cy)) = world.cursor_pos {
            let mut stroke_cur = Stroke::default();
            stroke_cur.width = 1.5;
            paint.set_color_rgba8(255, 215, 0, 200);

            let mut pb = PathBuilder::new();
            pb.move_to(cx - 8.0, cy);
            pb.line_to(cx + 8.0, cy);
            pb.move_to(cx, cy - 8.0);
            pb.line_to(cx, cy + 8.0);

            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint, &stroke_cur, Transform::identity(), None);
            }
        }
    }
}
