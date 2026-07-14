import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type AlertSoundInfo = {
  id: string;
  label: string;
};

export type AudioOutputDevice = {
  name: string;
  channels: number;
  is_default: boolean;
};

export type VoicePreview = {
  message: string;
  wav_base64: string;
};

let previewAudio: HTMLAudioElement | null = null;

export async function playWavBase64(wavBase64: string, volume = 1): Promise<void> {
  if (previewAudio) {
    previewAudio.pause();
    previewAudio.src = "";
    previewAudio = null;
  }
  const audio = new Audio(`data:audio/wav;base64,${wavBase64}`);
  let vol = volume;
  if (vol < 0) vol = 0;
  if (vol > 1) vol = 1;
  audio.volume = vol;
  previewAudio = audio;
  await audio.play().catch((err) => {
    console.error("audio.play failed", err);
    throw err;
  });
}

/** Listen for in-game alert TTS emitted from Rust. */
export async function bindTtsPlayback(): Promise<UnlistenFn> {
  return listen<{ wav_base64: string; volume?: number }>("tts-play", (event) => {
    void playWavBase64(event.payload.wav_base64, event.payload.volume ?? 1).catch(
      (err) => {
        console.error("tts-play failed", err);
      },
    );
  });
}

/** Synthesize via Kokoro and play (native path; WebView if WAV payload). */
export async function previewVoice(
  voiceId: string,
  text = "Out of mana",
  volume?: number,
): Promise<string> {
  const result = await invoke<VoicePreview>("preview_kokoro_voice", {
    voiceId,
    text,
    volume: volume ?? null,
  });
  if (result.wav_base64) {
    await playWavBase64(result.wav_base64, volume ?? 1);
  }
  return result.message;
}

export async function listAudioOutputDevices(): Promise<AudioOutputDevice[]> {
  return invoke<AudioOutputDevice[]>("list_audio_output_devices");
}

/** Speak alert text via native OS TTS (male/female from settings). */
export async function speakText(text: string): Promise<void> {
  const cleaned = text.trim();
  if (!cleaned) {
    return;
  }
  await invoke("speak_text", { text: cleaned });
}

/** Speak callout — inspector Test button (voice only; use Preview for chime). */
export async function testSpeech(text: string, _sound?: string): Promise<string> {
  return invoke<string>("test_speech", {
    text: text.trim() || "Alert test",
    sound: null,
  });
}

export async function listAlertSounds(): Promise<AlertSoundInfo[]> {
  return invoke<AlertSoundInfo[]>("list_alert_sounds");
}

export async function playAlertSound(sound: string): Promise<void> {
  await invoke("play_alert_sound", { sound });
}
