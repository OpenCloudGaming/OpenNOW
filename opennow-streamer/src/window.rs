use std::{
    collections::{HashMap, HashSet},
    mem::size_of,
    sync::mpsc::Receiver,
    time::Duration,
};

use tokio::sync::mpsc::UnboundedSender;

use anyhow::Context;
use sdl2::{
    audio::{AudioQueue, AudioSpecDesired},
    controller::Button as ControllerButton,
    event::Event,
    hint,
    keyboard::{Mod, Scancode},
    mouse::MouseUtil,
    pixels::PixelFormatEnum,
    rect::Rect,
    render::{BlendMode, Canvas, TextureCreator},
    video::Window,
    video::WindowContext,
};

use crate::{
    media::{AudioFrame, MediaEvent, VideoFrame, VideoPixelFormat},
    session::{InputPayload, SharedSession},
};

pub fn run(
    _session: SharedSession,
    media_rx: Receiver<MediaEvent>,
    input_tx: UnboundedSender<InputPayload>,
    width: u32,
    height: u32,
) -> anyhow::Result<()> {
    hint::set("SDL_MOUSE_AUTO_CAPTURE", "0");
    hint::set("SDL_MOUSE_FOCUS_CLICKTHROUGH", "1");
    hint::set("SDL_MOUSE_RELATIVE_MODE_WARP", "0");
    hint::set("SDL_MOUSE_RELATIVE_WARP_MOTION", "0");
    hint::set("SDL_MOUSE_RELATIVE_SYSTEM_SCALE", "0");
    hint::set("SDL_MOUSE_RELATIVE_SPEED_SCALE", "1");
    hint::set("SDL_MOUSE_TOUCH_EVENTS", "0");
    hint::set_video_minimize_on_focus_loss(false);

    let sdl = sdl2::init()
        .map_err(|e| anyhow::anyhow!(e))
        .context("sdl init")?;
    let video = sdl
        .video()
        .map_err(|e| anyhow::anyhow!(e))
        .context("sdl video")?;
    let audio = sdl
        .audio()
        .map_err(|e| anyhow::anyhow!(e))
        .context("sdl audio")?;
    let mouse = sdl.mouse();
    let game_controller = sdl.game_controller().ok();
    let mut opened_controllers: HashMap<u32, sdl2::controller::GameController> = HashMap::new();
    if let Some(gc) = &game_controller {
        for idx in 0_u32..gc.num_joysticks().unwrap_or(0) {
            if gc.is_game_controller(idx) {
                if let Ok(controller) = gc.open(idx) {
                    opened_controllers.insert(controller.instance_id(), controller);
                }
            }
        }
    }

    let window = video
        .window("OpenNOW Streamer", width.max(640), height.max(360))
        .position_centered()
        .resizable()
        .build()
        .context("create SDL window")?;
    let mut canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .context("create SDL canvas")?;
    let texture_creator: TextureCreator<WindowContext> = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_streaming(PixelFormatEnum::NV12, width, height)
        .context("create texture")?;
    texture.set_blend_mode(BlendMode::None);
    let mut texture_width = width;
    let mut texture_height = height;
    let mut texture_format = VideoPixelFormat::Nv12;

    let queue: AudioQueue<i16> = audio
        .open_queue::<i16, _>(
            None,
            &AudioSpecDesired {
                freq: Some(48_000),
                channels: Some(2),
                samples: Some(1024),
            },
        )
        .map_err(|e| anyhow::anyhow!(e))
        .context("open SDL audio queue")?;
    queue.resume();

    let mut event_pump = sdl
        .event_pump()
        .map_err(|e| anyhow::anyhow!(e))
        .context("event pump")?;
    let mut latest_frame: Option<VideoFrame> = None;
    let mut connected_slots = HashSet::<u8>::new();
    let mut mouse_captured = false;
    let mut running = true;
    let mut frame_dirty = false;
    while running {
        while let Ok(event) = media_rx.try_recv() {
            match event {
                MediaEvent::Video(frame) => {
                    latest_frame = Some(frame);
                    frame_dirty = true;
                }
                MediaEvent::Audio(frame) => queue_audio(&queue, frame),
            }
        }

        if let Some(frame) = latest_frame.take() {
            if frame.width != texture_width
                || frame.height != texture_height
                || frame.pixel_format != texture_format
            {
                let pixel_format = match frame.pixel_format {
                    VideoPixelFormat::I420 => PixelFormatEnum::IYUV,
                    VideoPixelFormat::Nv12 => PixelFormatEnum::NV12,
                };
                texture = texture_creator
                    .create_texture_streaming(pixel_format, frame.width, frame.height)
                    .context("recreate texture")?;
                texture.set_blend_mode(BlendMode::None);
                texture_width = frame.width;
                texture_height = frame.height;
                texture_format = frame.pixel_format;
                let window = canvas.window_mut();
                let _ = window.set_size(frame.width.max(640), frame.height.max(360));
            }
            match frame.pixel_format {
                VideoPixelFormat::I420 => {
                    let y_len = (frame.width * frame.height) as usize;
                    let uv_len = ((frame.width / 2) * (frame.height / 2)) as usize;
                    let y_plane = &frame.data[..y_len];
                    let u_plane = &frame.data[y_len..(y_len + uv_len)];
                    let v_plane = &frame.data[(y_len + uv_len)..(y_len + uv_len + uv_len)];
                    texture
                        .update_yuv(
                            None,
                            y_plane,
                            frame.width as usize,
                            u_plane,
                            (frame.width / 2) as usize,
                            v_plane,
                            (frame.width / 2) as usize,
                        )
                        .context("texture update yuv")?;
                }
                VideoPixelFormat::Nv12 => {
                    texture
                        .update(None, &frame.data, frame.width as usize)
                        .context("texture update nv12")?;
                }
            }
        }

        if frame_dirty {
            canvas.clear();
            let (out_w, out_h) = canvas.output_size().unwrap_or((width, height));
            canvas
                .copy(&texture, None, Some(Rect::new(0, 0, out_w, out_h)))
                .ok();
            canvas.present();
            frame_dirty = false;
        }

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => running = false,
                Event::Window { win_event, .. } => match win_event {
                    sdl2::event::WindowEvent::FocusLost
                    | sdl2::event::WindowEvent::Leave
                    | sdl2::event::WindowEvent::Close => {
                        if mouse_captured {
                            set_mouse_capture(&mouse, &mut canvas, false);
                            mouse_captured = false;
                        }
                    }
                    _ => {}
                },
                Event::KeyDown {
                    scancode: Some(Scancode::Escape),
                    repeat: false,
                    ..
                } => {
                    if mouse_captured {
                        set_mouse_capture(&mouse, &mut canvas, false);
                        mouse_captured = false;
                    } else if let Some((vk, code)) = map_scancode(Scancode::Escape) {
                        send_input(
                            &input_tx,
                            InputPayload::Key {
                                key_code: vk,
                                scan_code: code,
                                modifiers: 0,
                                down: true,
                            },
                        );
                    }
                }
                Event::KeyDown {
                    scancode: Some(scancode),
                    keymod,
                    repeat,
                    ..
                } => {
                    if !repeat {
                        if let Some((vk, code)) = map_scancode(scancode) {
                            send_input(
                                &input_tx,
                                InputPayload::Key {
                                    key_code: vk,
                                    scan_code: code,
                                    modifiers: map_modifiers(keymod),
                                    down: true,
                                },
                            );
                        }
                    }
                }
                Event::KeyUp {
                    scancode: Some(scancode),
                    keymod,
                    repeat,
                    ..
                } => {
                    if !repeat {
                        if let Some((vk, code)) = map_scancode(scancode) {
                            send_input(
                                &input_tx,
                                InputPayload::Key {
                                    key_code: vk,
                                    scan_code: code,
                                    modifiers: map_modifiers(keymod),
                                    down: false,
                                },
                            );
                        }
                    }
                }
                Event::MouseMotion { xrel, yrel, .. } => {
                    if mouse_captured && (xrel != 0 || yrel != 0) {
                        send_input(
                            &input_tx,
                            InputPayload::MouseMove {
                                dx: xrel.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                                dy: yrel.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                            },
                        );
                    }
                }
                Event::MouseButtonDown { mouse_btn, .. } => {
                    if !mouse_captured {
                        set_mouse_capture(&mouse, &mut canvas, true);
                        mouse_captured = true;
                    }
                    if let Some(button) = map_mouse_button(mouse_btn) {
                        send_input(&input_tx, InputPayload::MouseButton { button, down: true });
                    }
                }
                Event::MouseButtonUp { mouse_btn, .. } => {
                    if mouse_captured {
                        if let Some(button) = map_mouse_button(mouse_btn) {
                            send_input(
                                &input_tx,
                                InputPayload::MouseButton {
                                    button,
                                    down: false,
                                },
                            );
                        }
                    }
                }
                Event::MouseWheel { y, .. } => {
                    if mouse_captured {
                        let delta = (-y).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                        send_input(&input_tx, InputPayload::MouseWheel { delta });
                    }
                }
                Event::ControllerDeviceAdded { which, .. } => {
                    if let Some(gc) = &game_controller {
                        if let Ok(controller) = gc.open(which) {
                            let instance_id = controller.instance_id();
                            opened_controllers.insert(instance_id, controller);
                            let controller_id = slot_for_instance(instance_id, &opened_controllers);
                            connected_slots.insert(controller_id);
                            send_controller_state(
                                &input_tx,
                                &opened_controllers,
                                controller_id,
                                Some(instance_id),
                                &connected_slots,
                            );
                        }
                    }
                }
                Event::ControllerDeviceRemoved { which, .. } => {
                    let controller_id = slot_for_instance(which, &opened_controllers);
                    opened_controllers.remove(&which);
                    connected_slots.remove(&controller_id);
                    send_controller_state(
                        &input_tx,
                        &opened_controllers,
                        controller_id,
                        None,
                        &connected_slots,
                    );
                }
                Event::ControllerAxisMotion { which, .. }
                | Event::ControllerButtonDown { which, .. }
                | Event::ControllerButtonUp { which, .. } => {
                    if opened_controllers.contains_key(&which) {
                        let controller_id = slot_for_instance(which, &opened_controllers);
                        connected_slots.insert(controller_id);
                        send_controller_state(
                            &input_tx,
                            &opened_controllers,
                            controller_id,
                            Some(which),
                            &connected_slots,
                        );
                    }
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }

    Ok(())
}

fn set_mouse_capture(mouse: &MouseUtil, canvas: &mut Canvas<Window>, captured: bool) {
    mouse.capture(captured);
    mouse.set_relative_mouse_mode(captured);
    mouse.show_cursor(!captured);
    let window = canvas.window_mut();
    window.set_grab(captured);
    window.set_mouse_grab(captured);
    window.set_keyboard_grab(captured);
}

fn send_input(input_tx: &UnboundedSender<InputPayload>, payload: InputPayload) {
    let _ = input_tx.send(payload);
}

fn send_controller_state(
    input_tx: &UnboundedSender<InputPayload>,
    controllers: &HashMap<u32, sdl2::controller::GameController>,
    controller_id: u8,
    instance_id: Option<u32>,
    connected_slots: &HashSet<u8>,
) {
    let bitmap = connected_slots
        .iter()
        .fold(0_u16, |bitmap, slot| bitmap | (1_u16 << slot));
    if let Some(instance_id) = instance_id {
        if let Some(controller) = controllers.get(&instance_id) {
            send_input(
                input_tx,
                InputPayload::Gamepad {
                    controller_id,
                    bitmap,
                    buttons: map_controller_buttons(controller),
                    left_trigger: axis_to_u8(controller.axis(sdl2::controller::Axis::TriggerLeft)),
                    right_trigger: axis_to_u8(
                        controller.axis(sdl2::controller::Axis::TriggerRight),
                    ),
                    left_x: controller.axis(sdl2::controller::Axis::LeftX),
                    left_y: -controller.axis(sdl2::controller::Axis::LeftY),
                    right_x: controller.axis(sdl2::controller::Axis::RightX),
                    right_y: -controller.axis(sdl2::controller::Axis::RightY),
                },
            );
            return;
        }
    }
    send_input(
        input_tx,
        InputPayload::Gamepad {
            controller_id,
            bitmap,
            buttons: 0,
            left_trigger: 0,
            right_trigger: 0,
            left_x: 0,
            left_y: 0,
            right_x: 0,
            right_y: 0,
        },
    );
}

fn slot_for_instance(
    instance_id: u32,
    controllers: &HashMap<u32, sdl2::controller::GameController>,
) -> u8 {
    let mut sorted = controllers.keys().copied().collect::<Vec<_>>();
    sorted.sort_unstable();
    sorted
        .iter()
        .position(|value| *value == instance_id)
        .unwrap_or(0)
        .min(3) as u8
}

fn queue_audio(queue: &AudioQueue<i16>, frame: AudioFrame) {
    let bytes_per_sample = size_of::<i16>() as u32;
    let bytes_per_second = frame
        .sample_rate
        .saturating_mul(frame.channels.max(1) as u32)
        .saturating_mul(bytes_per_sample);
    let max_queued_bytes =
        (bytes_per_second / 20).max(bytes_per_sample * frame.samples.len() as u32);
    if queue.size() > max_queued_bytes {
        queue.clear();
    }
    let _ = queue.queue_audio(&frame.samples);
}

fn map_modifiers(mods: Mod) -> u16 {
    let mut flags = 0_u16;
    if mods.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD) {
        flags |= 0x01;
    }
    if mods.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) {
        flags |= 0x02;
    }
    if mods.intersects(Mod::LALTMOD | Mod::RALTMOD) {
        flags |= 0x04;
    }
    if mods.intersects(Mod::LGUIMOD | Mod::RGUIMOD) {
        flags |= 0x08;
    }
    if mods.intersects(Mod::CAPSMOD) {
        flags |= 0x10;
    }
    if mods.intersects(Mod::NUMMOD) {
        flags |= 0x20;
    }
    flags
}

fn map_mouse_button(button: sdl2::mouse::MouseButton) -> Option<u8> {
    Some(match button {
        sdl2::mouse::MouseButton::Left => 1,
        sdl2::mouse::MouseButton::Middle => 2,
        sdl2::mouse::MouseButton::Right => 3,
        sdl2::mouse::MouseButton::X1 => 4,
        sdl2::mouse::MouseButton::X2 => 5,
        _ => return None,
    })
}

fn map_controller_buttons(controller: &sdl2::controller::GameController) -> u16 {
    let mut buttons = 0_u16;
    if controller.button(ControllerButton::DPadUp) {
        buttons |= 0x0001;
    }
    if controller.button(ControllerButton::DPadDown) {
        buttons |= 0x0002;
    }
    if controller.button(ControllerButton::DPadLeft) {
        buttons |= 0x0004;
    }
    if controller.button(ControllerButton::DPadRight) {
        buttons |= 0x0008;
    }
    if controller.button(ControllerButton::Start) {
        buttons |= 0x0010;
    }
    if controller.button(ControllerButton::Back) {
        buttons |= 0x0020;
    }
    if controller.button(ControllerButton::LeftStick) {
        buttons |= 0x0040;
    }
    if controller.button(ControllerButton::RightStick) {
        buttons |= 0x0080;
    }
    if controller.button(ControllerButton::LeftShoulder) {
        buttons |= 0x0100;
    }
    if controller.button(ControllerButton::RightShoulder) {
        buttons |= 0x0200;
    }
    if controller.button(ControllerButton::Guide) {
        buttons |= 0x0400;
    }
    if controller.button(ControllerButton::A) {
        buttons |= 0x1000;
    }
    if controller.button(ControllerButton::B) {
        buttons |= 0x2000;
    }
    if controller.button(ControllerButton::X) {
        buttons |= 0x4000;
    }
    if controller.button(ControllerButton::Y) {
        buttons |= 0x8000;
    }
    buttons
}

fn axis_to_u8(value: i16) -> u8 {
    (((value.max(0) as f32) / 32767.0) * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn map_scancode(code: Scancode) -> Option<(u16, u16)> {
    Some(match code {
        Scancode::A => (0x41, 0x04),
        Scancode::B => (0x42, 0x05),
        Scancode::C => (0x43, 0x06),
        Scancode::D => (0x44, 0x07),
        Scancode::E => (0x45, 0x08),
        Scancode::F => (0x46, 0x09),
        Scancode::G => (0x47, 0x0A),
        Scancode::H => (0x48, 0x0B),
        Scancode::I => (0x49, 0x0C),
        Scancode::J => (0x4A, 0x0D),
        Scancode::K => (0x4B, 0x0E),
        Scancode::L => (0x4C, 0x0F),
        Scancode::M => (0x4D, 0x10),
        Scancode::N => (0x4E, 0x11),
        Scancode::O => (0x4F, 0x12),
        Scancode::P => (0x50, 0x13),
        Scancode::Q => (0x51, 0x14),
        Scancode::R => (0x52, 0x15),
        Scancode::S => (0x53, 0x16),
        Scancode::T => (0x54, 0x17),
        Scancode::U => (0x55, 0x18),
        Scancode::V => (0x56, 0x19),
        Scancode::W => (0x57, 0x1A),
        Scancode::X => (0x58, 0x1B),
        Scancode::Y => (0x59, 0x1C),
        Scancode::Z => (0x5A, 0x1D),
        Scancode::Num1 => (0x31, 0x1E),
        Scancode::Num2 => (0x32, 0x1F),
        Scancode::Num3 => (0x33, 0x20),
        Scancode::Num4 => (0x34, 0x21),
        Scancode::Num5 => (0x35, 0x22),
        Scancode::Num6 => (0x36, 0x23),
        Scancode::Num7 => (0x37, 0x24),
        Scancode::Num8 => (0x38, 0x25),
        Scancode::Num9 => (0x39, 0x26),
        Scancode::Num0 => (0x30, 0x27),
        Scancode::Return => (0x0D, 0x28),
        Scancode::Escape => (0x1B, 0x29),
        Scancode::Backspace => (0x08, 0x2A),
        Scancode::Tab => (0x09, 0x2B),
        Scancode::Space => (0x20, 0x2C),
        Scancode::Left => (0x25, 0x50),
        Scancode::Right => (0x27, 0x4F),
        Scancode::Up => (0x26, 0x52),
        Scancode::Down => (0x28, 0x51),
        Scancode::LShift => (0xA0, 0xE1),
        Scancode::RShift => (0xA1, 0xE5),
        Scancode::LCtrl => (0xA2, 0xE0),
        Scancode::RCtrl => (0xA3, 0xE4),
        Scancode::LAlt => (0xA4, 0xE2),
        Scancode::RAlt => (0xA5, 0xE6),
        Scancode::LGui => (0x5B, 0xE3),
        Scancode::RGui => (0x5C, 0xE7),
        Scancode::F1 => (0x70, 0x3A),
        Scancode::F2 => (0x71, 0x3B),
        Scancode::F3 => (0x72, 0x3C),
        Scancode::F4 => (0x73, 0x3D),
        Scancode::F5 => (0x74, 0x3E),
        Scancode::F6 => (0x75, 0x3F),
        Scancode::F7 => (0x76, 0x40),
        Scancode::F8 => (0x77, 0x41),
        Scancode::F9 => (0x78, 0x42),
        Scancode::F10 => (0x79, 0x43),
        Scancode::F11 => (0x7A, 0x44),
        Scancode::F12 => (0x7B, 0x45),
        _ => return None,
    })
}
