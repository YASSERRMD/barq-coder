pub mod stt;
pub mod tts;
pub mod vad;

pub struct VoiceConfig {
    pub stt_model: String,
    pub tts_voice: String,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            stt_model: "base".to_string(),
            tts_voice: "default".to_string(),
        }
    }
}
