pub mod phonon;
pub mod playback;
pub mod queue;

use phonon::{PhononManager, PhononStatusInfo};
use playback::AudioPlayer;
use queue::SpeechQueue;
use std::sync::Arc;
use tauri::State;

pub struct VoiceState {
    pub phonon: Arc<PhononManager>,
    pub player: AudioPlayer,
    pub queue: SpeechQueue,
}

#[tauri::command]
pub async fn get_phonon_status(state: State<'_, VoiceState>) -> Result<PhononStatusInfo, String> {
    Ok(state.phonon.refresh_status().await)
}

#[tauri::command]
pub async fn start_phonon(state: State<'_, VoiceState>) -> Result<(), String> {
    state.phonon.start_server(None).await
}

#[tauri::command]
pub async fn install_uv(state: State<'_, VoiceState>) -> Result<(), String> {
    state.phonon.install_uv().await
}

#[tauri::command]
pub async fn install_ptts(state: State<'_, VoiceState>) -> Result<(), String> {
    state.phonon.install_ptts().await
}

#[tauri::command]
pub async fn stop_speech(state: State<'_, VoiceState>) -> Result<(), String> {
    state.queue.stop_speech().await;
    Ok(())
}

#[tauri::command]
pub async fn set_voice_volume(volume: f32, state: State<'_, VoiceState>) -> Result<(), String> {
    state.player.set_volume(volume);
    // Also save to config file
    if let Ok(mut cfg) = crate::config::load_or_create_config() {
        cfg.voice.volume = volume.clamp(0.0, 1.0);
        let _ = crate::config::save_config(&cfg);
    }
    Ok(())
}

#[tauri::command]
pub async fn set_voice_enabled(enabled: bool, state: State<'_, VoiceState>) -> Result<(), String> {
    if !enabled {
        state.player.set_volume(0.0);
    } else if let Ok(cfg) = crate::config::load_or_create_config() {
        state.player.set_volume(cfg.voice.volume);
    }

    if let Ok(mut cfg) = crate::config::load_or_create_config() {
        cfg.voice.enabled = enabled;
        let _ = crate::config::save_config(&cfg);
    }
    Ok(())
}

#[tauri::command]
pub async fn set_agent_voice(agent_kind: String, voice: String, _state: State<'_, VoiceState>) -> Result<(), String> {
    if let Ok(mut cfg) = crate::config::load_or_create_config() {
        cfg.voice.voices.insert(agent_kind, voice);
        let _ = crate::config::save_config(&cfg);
    }
    Ok(())
}
